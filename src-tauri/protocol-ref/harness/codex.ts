import type { HarnessAdapter, HarnessState } from "./types.ts";

// Only OSC title signals; never infer completion from silence or screen text.
// The launch override excludes project/thread names so these words cannot collide
// with user-controlled titles. Spinner also enables Codex's Action Required title.
export const codexAdapter: HarnessAdapter = {
  id: "codex",
  executable: "codex",
  args: [
    "-c", 'tui.terminal_title=["app-name","status","spinner","session-id"]',
    // Plan / request_user_input is not a hook. OSC 9 is the only external signal.
    "-c", 'tui.notifications=["plan-mode-prompt","approval-requested"]',
    "-c", 'tui.notification_method="osc9"',
    "-c", 'tui.notification_condition="always"',
  ],
  resumeArgs(sessionId) {
    if (!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(sessionId)) throw new Error("无效的 Codex 会话 ID，无法续接");
    return ["resume", sessionId];
  },
  createProbe() {
    let pending = "";
    let sessionId: string | undefined;
    let sessionIdPrefix: string | undefined;
    let needsInput = false;
    return {
      get sessionId() { return sessionId; },
      get sessionIdPrefix() { return sessionIdPrefix; },
      consumeNeedsInput() {
        const waiting = needsInput;
        needsInput = false;
        return waiting;
      },
      push(data) {
      pending += data;
      let state: HarnessState | undefined;
      for (;;) {
        const start = pending.indexOf("\x1b]");
        if (start < 0) { pending = pending.endsWith("\x1b") ? "\x1b" : ""; break; }
        pending = pending.slice(start);
        const end = /\x07|\x1b\\/.exec(pending);
        if (!end) { if (pending.length > 4096) pending = ""; break; }
        const osc = pending.slice(2, end.index);
        pending = pending.slice(end.index + end[0].length);
        if (/^9;/.test(osc)) {
          needsInput = true;
          state = "attention";
          continue;
        }
        if (!/^[02];/.test(osc)) continue;
        const title = osc.slice(2);
        if (!/^(?:\[ ! \] Action Required(?: \|)? )?codex(?:\s|$)/.test(title)) continue;
        sessionIdPrefix = title.match(/\b([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{5})\.\.\./i)?.[1];
        sessionId = title.match(/\b[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\b/i)?.[0];
        if (/Action Required/.test(title)) {
          needsInput = true;
          state = "attention";
        }
        else if (/\b(Working|Thinking|Waiting)\b/.test(title)) state = "working";
        else if (/\bReady\b/.test(title)) state = "attention";
        else if (/\bStarting\b/.test(title)) state = "starting";
        else state = "unknown";
      }
      return state;
    } };
  },
};

// Codex's exit footer follows its terminal-title reset. Only use on exited PTYs,
// and verify persistence separately before offering resume.
export function codexExitSessionId(output: string): string | undefined {
  return output.replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "").match(/\x1b\]0;(?:\x07|\x1b\\)Session ID: ([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\r?\n\s*$/i)?.[1];
}
