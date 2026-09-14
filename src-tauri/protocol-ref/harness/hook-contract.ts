import type { HarnessState } from "./types.ts";

/** Environment variables injected into a TopCard terminal for the shared hook API. */
export const HOOK_ENV = {
  SIGNAL_DIR: "TOPCARD_HARNESS_SIGNAL_DIR",
  CHANNEL: "TOPCARD_HARNESS_CHANNEL",
  TTY: "TOPCARD_HARNESS_TTY",
  KIND: "TOPCARD_HARNESS_KIND",
  SESSION_ID: "TOPCARD_HARNESS_SESSION_ID",
  HOOK: "TOPCARD_HOOK",
} as const;

/**
 * Public lifecycle events for the shared hook API.
 * Built-in CLI adapters may emit aliases; normalizeHookSignal maps them here.
 */
export const CANONICAL_HOOK_EVENTS = [
  "SessionStart",
  "UserPromptSubmit",
  "Stop",
  "PermissionRequest",
  "SessionInfo",
] as const;

export type CanonicalHookEvent = (typeof CANONICAL_HOOK_EVENTS)[number];

/** CLI-specific names folded into the shared contract before observeHook. */
export const HOOK_EVENT_ALIASES: Record<string, CanonicalHookEvent> = {
  sessionStart: "SessionStart",
  SessionStart: "SessionStart",
  beforeSubmitPrompt: "UserPromptSubmit",
  UserPromptSubmit: "UserPromptSubmit",
  BeforeAgent: "UserPromptSubmit",
  PreInvocation: "UserPromptSubmit",
  PreToolUse: "UserPromptSubmit",
  PostToolUse: "UserPromptSubmit",
  PostToolUseFailure: "UserPromptSubmit",
  BeforeTool: "UserPromptSubmit",
  AfterTool: "UserPromptSubmit",
  PostInvocation: "UserPromptSubmit",
  stop: "Stop",
  Stop: "Stop",
  StopFailure: "Stop",
  StopCancelled: "Stop",
  sessionEnd: "Stop",
  AfterAgent: "Stop",
  afterAgentResponse: "Stop",
  preToolUse: "UserPromptSubmit",
  postToolUse: "UserPromptSubmit",
  postToolUseFailure: "UserPromptSubmit",
  beforeShellExecution: "PermissionRequest",
  beforeMCPExecution: "PermissionRequest",
  PermissionRequest: "PermissionRequest",
  SessionInfo: "SessionInfo",
};

/** Cursor TUI user/project hooks. Permission events return ask, never allow. */
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
  if (event === "beforeShellExecution" || event === "beforeMCPExecution") return '{"permission":"ask"}';
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
}

export function isCanonicalHookEvent(event: string): event is CanonicalHookEvent {
  return (CANONICAL_HOOK_EVENTS as readonly string[]).includes(event);
}

/** Map adapter-specific event names onto the shared public contract. */
export function normalizeHookSignal<T extends HookSignal>(signal: T): T {
  const event = HOOK_EVENT_ALIASES[signal.event];
  if (!event || event === signal.event) return signal;
  return { ...signal, event };
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
    event: HOOK_EVENT_ALIASES[input.event] ?? input.event,
    sessionId,
    title: clean(input.title),
    prompt: clean(input.prompt),
    replyPreview: clean(input.replyPreview),
    tool: typeof input.tool === "string" ? input.tool : undefined,
    notification: typeof input.notification === "string" ? input.notification : undefined,
    agentId: typeof input.agentId === "string" ? input.agentId : undefined,
  };
}

export type { HarnessState };
