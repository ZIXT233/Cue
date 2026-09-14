import { localEnvironment, resolveLocalCommand } from "./local-environment";
import { windowsCommand } from "./windows-command";
import { bundledPiCommand } from "./bundled-pi";
import { prepareShell } from "./shell-launch";
import { createShellProbe } from "./shell-probe";
import { prepareHookLaunch } from "./hook-launch";
import { createHookOscProbe } from "./hook-osc";
import { codexExitSessionId } from "./codex";
import { readTerminalTranscript } from "../terminal-transcript";
import { codexSessionTitle, resolveCodexSessionPrefix, codexSessionExists } from "./codex-title";
import { readdir, readFile, unlink } from "node:fs/promises";
import { join } from "node:path";
import { observeHook, observeTitle, type ProbeState, type HookSignal } from "./signals";
import { normalizeHookSignal } from "./hook-contract";
import { notifyQueueChanged } from "../card-queue-live";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { createTerminal, getTerminalSnapshot } from "../terminal-manager";
import { getHarnessAdapter } from "./registry";
import { claudeHarnessEnv, readClaudeHarnessAuth } from "./claude-auth";
import { connectionArgs } from "../ssh-connection";
import { shellQuote, sshLoginExec, sshLoginCommand } from "../ssh-workspace";
import type { QueueWorkspace } from "../card-queue";
import type { HarnessSession } from "./types";

const exec = promisify(execFile);
const globalRuntime = globalThis as typeof globalThis & { __topcardHarnessProbes?: Map<string, ProbeState> };
const states = globalRuntime.__topcardHarnessProbes ??= new Map();

export async function launchHarness(kind: unknown, workspace: QueueWorkspace, resume?: HarnessSession): Promise<HarnessSession> {
  const adapter = getHarnessAdapter(kind);
  const terminalId = randomUUID().replaceAll("-", "");
  const signalDirectory = signalDir(terminalId);
  if (resume && !resume.providerSessionId) throw new Error("未捕获到原会话 ID，无法续接。请通过新会话入口创建新卡片。");
  let executable: string, args: string[] | string, version: string;
  let shellCommandNotifications = false;
  let hooks: { args: string[]; env: Record<string, string> } = { args: [], env: {} };
  if (adapter.id === "shell") {
    const shell = await prepareShell(workspace, signalDirectory);
    executable = shell.executable; args = shell.args; version = shell.version;
    hooks.env = shell.env;
    shellCommandNotifications = shell.commandNotifications;
  } else {
    let commandPath = adapter.executable;
    let commandPrefix: string[] = [];
    if (workspace.kind === "local") {
      let env = await localEnvironment();
      let resolved = await resolveLocalCommand(adapter.executable, env);
      if (!resolved) { env = await localEnvironment(true); resolved = await resolveLocalCommand(adapter.executable, env); }
      if (!resolved && adapter.id === "pi") {
        const bundled = await bundledPiCommand(env);
        resolved = bundled.executable;
        commandPrefix = bundled.args;
        env = bundled.env;
      }
      if (!resolved) throw new Error(`找不到 ${adapter.executable}：已读取用户 Shell 环境并检查常见安装目录`);
      commandPath = resolved;
      hooks.env = env as Record<string, string>;
    }
    const launchEnvironment = hooks.env;
    const versionCommand = `${adapter.executable} ${adapter.id === "grok" ? "version" : "--version"}`;
    try {
      if (workspace.kind === "ssh") version = (await sshLoginExec(workspace.sshHost!, versionCommand)).toString().trim();
      else if (process.platform === "win32") {
        const command = windowsCommand(commandPath, [...commandPrefix, adapter.id === "grok" ? "version" : "--version"]);
        version = (await exec(command.executable, command.args, { cwd: workspace.cwd, timeout: 10000, windowsHide: true, windowsVerbatimArguments: command.windowsVerbatimArguments, env: { ...process.env, ...launchEnvironment } })).stdout.trim();
      }
      else version = (await exec(commandPath, [...commandPrefix, adapter.id === "grok" ? "version" : "--version"], { cwd: workspace.cwd, timeout: 10000, env: { ...process.env, ...launchEnvironment } })).stdout.trim();
    } catch (error) {
      throw new Error(`${adapter.executable} 启动检测失败：${error instanceof Error ? error.message : String(error)}`);
    }
    if (adapter.id === "pi") {
      const match = version.match(/(?:^|\s)v?(\d+)\.(\d+)\.(\d+)/);
      if (!match || (Number(match[1]) === 0 && (Number(match[2]) < 80 || (Number(match[2]) === 80 && Number(match[3]) < 4)))) {
        throw new Error("Pi CLI 状态集成需要 Pi 0.80.4 或更新版本（agent_settled 事件），请先升级机器上的 Pi");
      }
    }
    hooks = await prepareHookLaunch(adapter.id, signalDirectory, workspace, terminalId);
    hooks.env = { ...launchEnvironment, ...hooks.env };
    if (adapter.id === "claude") {
      hooks.env = { ...hooks.env, ...claudeHarnessEnv(await readClaudeHarnessAuth()) };
    }
    if (resume?.providerSessionId) hooks.env.TOPCARD_HARNESS_SESSION_ID = resume.providerSessionId;
    const launchArgs = [...commandPrefix, ...(resume ? adapter.resumeArgs(resume.providerSessionId!) : []), ...adapter.args, ...hooks.args];
    const command = [adapter.executable, ...launchArgs].map(shellQuote).join(" ");
    if (workspace.kind === "ssh") {
      executable = "ssh";
      const exports = Object.entries(hooks.env).map(([key, value]) => `${key}=${shellQuote(value)}`).join(" ");
      args = [...await connectionArgs(workspace.sshHost!, { requestTty: true }), sshLoginCommand(`cd ${shellQuote(workspace.cwd)} && ${exports ? `export ${exports} && ` : ""}TOPCARD_HARNESS_TTY=$(tty) && export TOPCARD_HARNESS_TTY && exec ${command}`)];
    } else if (process.platform === "win32") {
      const launch = windowsCommand(commandPath, launchArgs);
      executable = launch.executable;
      // node-pty accepts a verbatim Windows command line as a string.
      args = launch.windowsVerbatimArguments ? launch.args.join(" ") : launch.args;
    } else {
      executable = commandPath;
      args = launchArgs;
    }
  }
  const probe = adapter.createProbe();
  states.set(terminalId, { state: adapter.id === "shell" ? "attention" : "starting", at: Date.now(), sessionId: resume?.providerSessionId, title: resume?.title });
  const observeRemote = createHookOscProbe(terminalId, signal => {
    const current = states.get(terminalId);
    if (!current || adapter.id === "shell") return;
    const next = observeHook(current, normalizeHookSignal(signal));
    states.set(terminalId, next);
    if (next.state !== current.state || next.hookSeen !== current.hookSeen) notifyQueueChanged("hook");
  });
  const observeShell = createShellProbe((running, exitCode) => {
    const current = states.get(terminalId)!;
    if (adapter.id !== "shell") return;
    states.set(terminalId, { ...current, shellCommandRunning: running,
      shellCommandStartedAt: running ? Date.now() : current.shellCommandStartedAt,
      shellExitCode: running ? undefined : exitCode });
    notifyQueueChanged("shell");
  });
  try {
    createTerminal(workspace.runtimeCwd, 100, 30, terminalId, { executable, args, persistent: true, env: hooks.env,
      onOutput(data) {
        if (adapter.id === "shell" && shellCommandNotifications) observeShell(data);
        observeRemote(data);
        const state = probe.push(data);
        const needsInput = probe.consumeNeedsInput?.() === true;
        if (!state && !needsInput) return;
        const current = states.get(terminalId)!;
        const raised = needsInput
          ? observeHook(current, { event: "PermissionRequest", at: Date.now(), sessionId: current.sessionId })
          : current;
        const titleIsFallback = adapter.id !== "codex" || !raised.hookSeen;
        states.set(terminalId, {
          ...observeTitle(raised, state ?? raised.state, Date.now(), adapter.id === "codex"),
          ...(titleIsFallback ? {
            sessionId: probe.sessionId ?? raised.sessionId,
            sessionIdPrefix: probe.sessionIdPrefix,
            ...((probe.sessionId || probe.sessionIdPrefix) ? { identityAt: Date.now() } : {}),
          } : {}),
        });
        const next = states.get(terminalId)!;
        if (next.state !== current.state || next.hookSeen !== current.hookSeen) notifyQueueChanged("hook");
      },
    });
  } catch (error) { states.delete(terminalId); throw error; }
  return { kind: adapter.id, shellCommandNotifications, terminalId, state: adapter.id === "shell" ? "attention" : "starting", version, remote: workspace.kind === "ssh", providerSessionId: resume?.providerSessionId, title: resume?.title ?? (adapter.id === "shell" ? workspace.name : undefined) };
}
const signalDir = (id: string) => join(process.env.TOPCARD_DATA_DIR || join(process.cwd(), ".topcard"), "harness-signals", id);
export async function harnessSnapshot(session: HarnessSession): Promise<HarnessSession> {
  let current = states.get(session.terminalId);
  if (current) {
    try {
      const directory = signalDir(session.terminalId);
      const files = (await readdir(directory)).filter(name => /^\d+-[a-f0-9-]+\.json$/.test(name)).sort().slice(0, 200);
      for (const file of files) {
        try {
          const signal = normalizeHookSignal(JSON.parse(await readFile(join(directory, file), "utf8")) as HookSignal);
          current = observeHook(states.get(session.terminalId) ?? current, signal);
          states.set(session.terminalId, current);
        } catch { /* Malformed probe input cannot interrupt the queue. */ }
        await unlink(join(directory, file)).catch(() => {});
      }
      current = states.get(session.terminalId) ?? current;
    } catch { /* Remote title-only probe, or retired launch. */ }
  }
  let providerSessionId = current?.sessionId ?? session.providerSessionId;
  if (session.kind === "codex" && current?.sessionIdPrefix) {
    const prefix = current.sessionIdPrefix.toLowerCase();
    // A title prefix can confirm an already known identity, but cannot identify
    // a new remote session. Never retain the old ID after an in-TUI switch.
    providerSessionId = session.remote
      ? (providerSessionId?.toLowerCase().startsWith(prefix) ? providerSessionId : undefined)
      : await resolveCodexSessionPrefix(current.sessionIdPrefix);
  }
  const terminal = getTerminalSnapshot(session.terminalId);
  let unpersistedSession = false;
  if (session.kind === "codex" && (!terminal || terminal.exited) && !session.remote) {
    const output = terminal?.output ?? readTerminalTranscript(session.terminalId)?.output ?? "";
    const footerId = codexExitSessionId(output);
    if (footerId) {
      const persisted = await codexSessionExists(footerId);
      providerSessionId = persisted ? footerId : undefined;
      unpersistedSession = persisted === false;
    }
  }
  const title = session.kind === "codex" && providerSessionId && !session.remote ? await codexSessionTitle(providerSessionId) : current?.title;
  const shellNotify = session.kind === "shell" && !!session.shellNotify && session.shellCommandStartedAt === current?.shellCommandStartedAt;
  return { ...session, ...(session.kind === "shell" ? { shellCommandStartedAt: current?.shellCommandStartedAt, shellCommandRunning: current?.shellCommandRunning, shellExitCode: current?.shellExitCode, shellNotify } : {}), state: !terminal ? "error" : terminal.exited ? "exited" : session.kind === "shell" ? (shellNotify && current?.shellCommandRunning ? "working" : "attention") : current?.state ?? "unknown",
    providerSessionId, unpersistedSession, replyPreview: current?.replyPreview,
    title: title ?? (providerSessionId === session.providerSessionId ? session.title : undefined),
    source: current?.source,
    probe: current?.hookSeen ? (current.titleSeen ? "hooks-and-title" : "hooks") : current?.titleSeen ? "title-only" : "unconfirmed",
    ...(terminal?.exitCode != null ? { exitCode: terminal.exitCode } : {}),
  };
}
