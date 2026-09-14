import type { HarnessAdapter } from "./types";

function resumeArgs(id: string) {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$/.test(id)) throw new Error("无效的会话 ID，无法续接");
  return ["--resume", id];
}
// Hooks own the state. Do not infer completion from arbitrary screen text or silence.
const createProbe = () => ({ push: () => undefined });
export const claudeAdapter: HarnessAdapter = { id: "claude", executable: "claude", args: [], resumeArgs, createProbe };
export const cursorAdapter: HarnessAdapter = { id: "cursor", executable: "cursor-agent", args: [], resumeArgs, createProbe };
export const shellAdapter: HarnessAdapter = { id: "shell", executable: "", args: [], resumeArgs: () => [], createProbe };

export const piAdapter: HarnessAdapter = { id: "pi", executable: "pi", args: [], resumeArgs: id => ["--session", resumeArgs(id)[1]], createProbe };
export const ompAdapter: HarnessAdapter = { id: "omp", executable: "omp", args: ["--allow-home"], resumeArgs, createProbe };

export const grokAdapter: HarnessAdapter = { id: "grok", executable: "grok", args: [], resumeArgs, createProbe };
export const geminiAdapter: HarnessAdapter = { id: "gemini", executable: "gemini", args: [], resumeArgs, createProbe };
export const opencodeAdapter: HarnessAdapter = { id: "opencode", executable: "opencode", args: [], resumeArgs: id => ["--session", resumeArgs(id)[1]], createProbe };

export const antigravityAdapter: HarnessAdapter = { id: "antigravity", executable: "agy", args: [], resumeArgs: id => ["--conversation", resumeArgs(id)[1]], createProbe };
