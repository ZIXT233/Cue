/** SSH transport adapted from Pi's MIT-licensed examples/extensions/ssh.ts.
 * The Pi engine stays local; built-in filesystem and shell tools run remotely.
 */
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { createBashTool, createReadTool, createWriteTool, createEditTool, createLsTool, createFindTool, createGrepTool, type ExtensionAPI, type BashOperations } from "@earendil-works/pi-coding-agent";

export const shellQuote = (value: string) => `'${value.replace(/'/g, `'"'"'`)}'`;
import { connectionArgs } from "./ssh-connection";

// SSH remote commands do not normally load interactive shell startup files.
// Load the user's PATH / version manager first, then use POSIX syntax for the
// actual launch command even when their login shell is zsh or fish.
export function sshLoginCommand(command: string): string {
  const run = `exec /bin/sh -c ${shellQuote(command)}`;
  return `/bin/sh -c ${shellQuote(`exec "\${SHELL:-/bin/sh}" -ilc ${shellQuote(run)}`)}`;
}

export async function sshLoginExec(host: string, command: string): Promise<Buffer> {
  const marker = `__TOPCARD_LOGIN_${randomUUID()}__`;
  const output = await sshExec(host, sshLoginCommand(`printf '%s' ${shellQuote(marker)}; ${command}`));
  const start = output.indexOf(marker);
  if (start < 0) throw new Error("远程 Shell 未执行检测命令，请检查 Shell 启动配置");
  return output.subarray(start + Buffer.byteLength(marker));
}

export async function sshExec(host: string, command: string, input?: string | Buffer): Promise<Buffer> {
  const args = await connectionArgs(host);
  return new Promise((resolve, reject) => {
    const child = spawn("ssh", [...args, command], { stdio: ["pipe", "pipe", "pipe"] });
    const chunks: Buffer[] = [], errors: Buffer[] = [];
    const timeout = setTimeout(() => child.kill(), 20_000);
    child.stdin.on("error", () => {});
    child.stdin.end(input);
    child.stdout.on("data", (data) => chunks.push(data));
    child.stderr.on("data", (data) => errors.push(data));
    child.on("error", (error) => { clearTimeout(timeout); reject(error); });
    child.on("close", (code) => {
      clearTimeout(timeout);
      if (code !== 0) reject(new Error(`SSH 连接失败：${Buffer.concat(errors).toString().trim() || "连接超时"}`));
      else resolve(Buffer.concat(chunks));
    });
  });
}

export async function loadSshWorkspace(cwd: string): Promise<{ sshHost: string; cwd: string } | null> {
  // Only the shell's private runtime directories are interpreted as SSH roots.
  if (!cwd.startsWith(join(process.env.TOPCARD_DATA_DIR || join(process.cwd(), ".topcard"), "ssh") + "/")) return null;
  try { return JSON.parse(await readFile(join(cwd, "remote-workspace.json"), "utf8")); }
  catch (error) { throw new Error(`SSH 工作区配置不可读：${String(error)}`); }
}

export function createSshWorkspaceExtension(localCwd: string, remote: { sshHost: string; cwd: string }) {
  return (pi: ExtensionAPI) => {
    const mapPath = (path: string) => path === localCwd ? remote.cwd : path.startsWith(localCwd + "/") ? remote.cwd + path.slice(localCwd.length) : path;
    const exec = (command: string, input?: string) => sshExec(remote.sshHost, command, input);
    const readOps = {
      readFile: (path: string) => exec(`cat -- ${shellQuote(mapPath(path))}`),
      access: async (path: string) => { await exec(`test -r ${shellQuote(mapPath(path))}`); },
      detectImageMimeType: async (path: string) => {
        const mime = (await exec(`file --mime-type -b -- ${shellQuote(mapPath(path))}`)).toString().trim();
        return ["image/jpeg", "image/png", "image/gif", "image/webp"].includes(mime) ? mime : null;
      },
    };
    const writeOps = {
      writeFile: async (path: string, content: string) => { await exec(`cat > ${shellQuote(mapPath(path))}`, content); },
      mkdir: async (path: string) => { await exec(`mkdir -p -- ${shellQuote(mapPath(path))}`); },
    };
    const operations: BashOperations = {
      exec: async (command, cwd, { onData, signal, timeout }) => {
        const args = await connectionArgs(remote.sshHost);
        return new Promise((resolve, reject) => {
        if (signal?.aborted) { reject(new Error("aborted")); return; }
        const child = spawn("ssh", [...args, `cd ${shellQuote(mapPath(cwd))} && bash -c ${shellQuote(command)}`], { stdio: ["ignore", "pipe", "pipe"] });
        let timedOut = false;
        const timer = timeout ? setTimeout(() => { timedOut = true; child.kill(); }, timeout * 1000) : undefined;
        const abort = () => { child.kill(); };
        signal?.addEventListener("abort", abort, { once: true });
        child.stdout.on("data", onData); child.stderr.on("data", onData);
        const cleanup = () => { clearTimeout(timer); signal?.removeEventListener("abort", abort); };
        child.on("error", (error) => { cleanup(); reject(error); });
        child.on("close", (code) => { cleanup(); if (signal?.aborted) reject(new Error("aborted")); else if (timedOut) reject(new Error("SSH command timed out")); else resolve({ exitCode: code }); });
      }); },
    };
    const bash = createBashTool(localCwd, { operations, exposeSessionEnvironment: false });
    pi.registerTool(createReadTool(localCwd, { operations: readOps }));
    pi.registerTool(createWriteTool(localCwd, { operations: writeOps }));
    pi.registerTool(createEditTool(localCwd, { operations: { ...readOps, writeFile: writeOps.writeFile } }));
    pi.registerTool(bash);
    // Avoid falling back to local directories when users select the full preset.
    pi.registerTool({ ...createLsTool(localCwd), execute: (id, args, signal, update) => bash.execute(id, { command: `ls -la -- ${shellQuote(mapPath(args.path || localCwd))}` }, signal, update) });
    pi.registerTool({ ...createFindTool(localCwd), execute: (id, args, signal, update) => bash.execute(id, { command: `find ${shellQuote(mapPath(args.path || localCwd))} -path ${shellQuote(args.pattern)} -print | head -n ${Math.max(1, args.limit || 1000)}` }, signal, update) });
    pi.registerTool({ ...createGrepTool(localCwd), execute: (id, args, signal, update) => bash.execute(id, { command: `rg -n ${args.ignoreCase ? "-i " : ""}${args.literal ? "-F " : ""}${args.glob ? `-g ${shellQuote(args.glob)} ` : ""}${args.context ? `-C ${Math.max(0, args.context)} ` : ""}-m ${Math.max(1, args.limit || 100)} -- ${shellQuote(args.pattern)} ${shellQuote(mapPath(args.path || localCwd))}` }, signal, update) });
    pi.on("user_bash", () => ({ operations }));
    pi.on("before_agent_start", async (event) => ({ systemPrompt: `${event.systemPrompt}\n\nThis is an SSH workspace. Current working directory: ${remote.cwd} on ${remote.sshHost}. All built-in read/write/edit/bash/ls/find/grep tools target that remote machine. The local directory ${localCwd} only stores session metadata. Do not treat it as the project. Read remote AGENTS.md if present before editing. Third-party extensions still run in the local Pi runtime.` }));
  };
}
