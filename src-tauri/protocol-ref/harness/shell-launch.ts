import { mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { homedir } from "node:os";
import { shellQuote, sshExec } from "../ssh-workspace";
import { connectionArgs } from "../ssh-connection";
import type { QueueWorkspace } from "../card-queue";

export async function prepareShell(workspace: QueueWorkspace, directory: string) {
  const remote = workspace.kind === "ssh";
  let shell = process.env.SHELL || "/bin/bash";
  let root = directory;
  let userHome = homedir();
  let originalZdotdir = process.env.ZDOTDIR || userHome;
  if (remote) {
    const facts = (await sshExec(workspace.sshHost!, 'printf "%s\\n%s\\n%s" "$HOME" "${SHELL:-/bin/bash}" "${ZDOTDIR:-$HOME}"')).toString().split("\n");
    [userHome, shell, originalZdotdir] = facts;
    root = `${userHome}/.cache/topcard/shell/${basename(directory)}`;
  }
  const files: Record<string, string> = {};
  let args: string[];
  let commandNotifications = true;
  const env: Record<string, string> = {};
  if (!remote && process.platform === "win32") {
    shell = "powershell.exe";
    const script = await readFile(join(process.cwd(), "bin/shell/powershell-integration.ps1"), "utf8");
    args = ["-NoLogo", "-NoExit", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")];
  } else if (basename(shell) === "zsh") {
    files["integration.sh"] = await readFile(join(process.cwd(), "bin/shell/zsh-integration.sh"), "utf8");
    files[".zshenv"] = `ZDOTDIR=${shellQuote(originalZdotdir)}\n[[ -r "$ZDOTDIR/.zshenv" ]] && source "$ZDOTDIR/.zshenv"\nexport TOPCARD_USER_ZDOTDIR="\${ZDOTDIR:-$HOME}"\nexport ZDOTDIR=${shellQuote(root)}\n`;
    files[".zprofile"] = '[[ -r "$TOPCARD_USER_ZDOTDIR/.zprofile" ]] && source "$TOPCARD_USER_ZDOTDIR/.zprofile"\n';
    files[".zshrc"] = `ZDOTDIR="$TOPCARD_USER_ZDOTDIR"\n[[ -r "$ZDOTDIR/.zshrc" ]] && source "$ZDOTDIR/.zshrc"\nunset TOPCARD_USER_ZDOTDIR\nsource ${shellQuote(`${root}/integration.sh`)}\n`;
    env.ZDOTDIR = root;
    args = ["-il"];
  } else if (basename(shell) === "bash") {
    const integration = await readFile(join(process.cwd(), "bin/shell/bash-integration.sh"), "utf8");
    files["bashrc"] = `[[ -r "$HOME/.bashrc" ]] && source "$HOME/.bashrc"\n${integration}`;
    args = ["--rcfile", `${root}/bashrc`, "-i"];
  } else {
    // Ordinary shells remain usable without installing prompt/trap integration.
    args = ["-il"];
    commandNotifications = false;
  }
  if (remote) {
    // POSIX shell setup needs no remote Node.js or global dotfile edits.
    await sshExec(workspace.sshHost!, `umask 077; mkdir -p ${shellQuote(root)}`);
    for (const [name, body] of Object.entries(files)) await sshExec(workspace.sshHost!, `printf %s ${shellQuote(body)} > ${shellQuote(`${root}/${name}`)}`);
    const exports = Object.entries(env).map(([key, value]) => `${key}=${shellQuote(value)}`).join(" ");
    return { executable: "ssh", args: [...await connectionArgs(workspace.sshHost!, { requestTty: true }), `cd ${shellQuote(workspace.cwd)} && ${exports ? `export ${exports} && ` : ""}exec ${[shell, ...args].map(shellQuote).join(" ")}`], env: {}, version: "", commandNotifications };
  }
  for (const [name, body] of Object.entries(files)) { await mkdir(root, { recursive: true, mode: 0o700 }); await writeFile(join(root, name), body, { mode: 0o600 }); }
  return { executable: shell, args, env, version: "", commandNotifications };
}
