import type { HarnessState } from "./types.ts";

/** Environment variables injected into a Cue terminal for the shared hook API. */
export const HOOK_ENV = {
  SIGNAL_DIR: "CUE_HARNESS_SIGNAL_DIR",
  CHANNEL: "CUE_HARNESS_CHANNEL",
  TTY: "CUE_HARNESS_TTY",
  KIND: "CUE_HARNESS_KIND",
  SESSION_ID: "CUE_HARNESS_SESSION_ID",
  HOOK: "CUE_HOOK",
} as const;

/**
 * A harness's own event names are read in exactly one place — `meaningOf` in
 * `signals.ts` — and turn straight into what a card does with them. There is
 * deliberately no intermediate vocabulary of "canonical events": such a layer only
 * produced a name that had to be translated again, and it invited filing an event under
 * a meaning it does not have.
 */

/** Cursor TUI user/project hooks. Permission events return allow: observation never gates the session. */
export const CURSOR_HOOK_EVENTS = [
  "sessionStart",
  "beforeSubmitPrompt",
  "preToolUse",
  "postToolUse",
  "postToolUseFailure",
  "beforeShellExecution",
  "beforeMCPExecution",
  "afterAgentResponse",
  "stop",
  "sessionEnd",
] as const;

export function cursorHookStdout(event?: string): string {
  if (event === "beforeSubmitPrompt") return '{"continue":true}';
  if (event === "beforeShellExecution" || event === "beforeMCPExecution") return '{"permission":"allow"}';
  return "{}";
}

export interface HookSignal {
  kind?: string;
  at: number;
  event: string;
  replyPreview?: string;
  sessionId?: string;
  agentId?: string;
  tool?: string;
  prompt?: string;
  title?: string;
  notification?: string;
  /** Antigravity: whether a `Stop` really ended the turn (`false` = paused mid-turn). */
  fullyIdle?: boolean;
}

export function buildHookSignal(input: {
  event: string;
  kind?: string;
  sessionId?: string;
  title?: string;
  prompt?: string;
  replyPreview?: string;
  tool?: string;
  notification?: string;
  agentId?: string;
  fullyIdle?: boolean;
  at?: number;
}): HookSignal {
  const clean = (value: unknown) => typeof value === "string"
    ? value.replace(/[\x00-\x1f\x7f]/g, " ").trim().slice(0, 160) || undefined
    : undefined;
  const sessionId = typeof input.sessionId === "string" && /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$/.test(input.sessionId)
    ? input.sessionId
    : undefined;
  return {
    kind: input.kind,
    at: Number.isFinite(input.at) ? Number(input.at) : Date.now(),
    event: input.event,
    sessionId,
    title: clean(input.title),
    prompt: clean(input.prompt),
    replyPreview: clean(input.replyPreview),
    tool: typeof input.tool === "string" ? input.tool : undefined,
    notification: typeof input.notification === "string" ? input.notification : undefined,
    agentId: typeof input.agentId === "string" ? input.agentId : undefined,
    fullyIdle: typeof input.fullyIdle === "boolean" ? input.fullyIdle : undefined,
  };
}

export type { HarnessState };
