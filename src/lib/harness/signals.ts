import type { HarnessState } from "./types.ts";
import { CURSOR_HOOK_EVENTS, normalizeHookSignal, type HookSignal } from "./hook-contract.ts";
export type { HookSignal } from "./hook-contract.ts";

export interface ProbeState {
  replyPreview?: string;
  antigravityCompleted?: boolean;
  state: HarnessState;
  at: number;
  shellCommandStartedAt?: number;
  shellCommandRunning?: boolean;
  shellExitCode?: number;
  sessionName?: string;
  firstPrompt?: string;
  /** Last hooked submit on this card. */
  submitPrompt?: string;
  titleState?: HarnessState;
  titleSeen?: boolean;
  hookSeen?: boolean;
  sessionId?: string;
  identityAt?: number;
  sessionIdPrefix?: string;
  source?: "hook" | "title";
}

// Event mapping follows Orca's Codex adapter (MIT, attribution in docs/harness).
// TopCard schedules the interactive root TUI, not a roster of background agents.
export function hookState(event: HookSignal): HarnessState | undefined {
  const signal = normalizeHookSignal(event);
  if (signal.agentId) return undefined;
  const notification = signal.notification ?? event.notification;
  if ((signal.event === "Notification" || event.event === "Notification") && ["permission_prompt", "ToolPermission", "idle_prompt"].includes(notification ?? "")) {
    return "attention";
  }
  if (signal.event === "PermissionRequest") return "attention";
  const asking = /(^|[/.])(request_user_input|ask_user_question|AskUserQuestion)$/.test(signal.tool ?? event.tool ?? "");
  if (asking && ["PreToolUse", "BeforeTool", "preToolUse"].includes(event.event)) return "attention";
  if (signal.event === "UserPromptSubmit") return "working";
  if (signal.event === "Stop") return "attention";
  return undefined;
}

const identityStartEvents = new Set(["SessionStart", "sessionStart", "PreInvocation"]);
const cursorEvents = new Set<string>(CURSOR_HOOK_EVENTS);

export function observeHook(current: ProbeState, raw: HookSignal): ProbeState {
  const signal = normalizeHookSignal(raw);
  if (!Number.isFinite(signal.at) || signal.agentId) return current;
  if (typeof signal.event !== "string") return current;
  const sessionId = typeof signal.sessionId === "string" && /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$/.test(signal.sessionId) ? signal.sessionId : undefined;
  const identityStart = identityStartEvents.has(signal.event) || identityStartEvents.has(raw.event);
  // Cursor user hooks are global: IDE/other chats must not steal this card's identity
  // or push it into working before our own sessionStart binds the launch.
  if (sessionId && current.sessionId && sessionId !== current.sessionId && !identityStart) return current;
  if ((signal.kind === "cursor" || raw.kind === "cursor" || cursorEvents.has(raw.event)) && sessionId && !current.sessionId && !identityStart) return current;
  const cleanTitle = (value: unknown) => typeof value === "string" ? value.replace(/[\x00-\x1f\x7f]/g, " ").trim().slice(0, 160) || undefined : undefined;
  const newIdentity = sessionId && sessionId !== current.sessionId;
  const submitEvent = signal.event === "UserPromptSubmit" || ["beforeSubmitPrompt", "UserPromptSubmit", "BeforeAgent"].includes(raw.event);
  const sessionName = cleanTitle(signal.title) ?? (newIdentity ? undefined : current.sessionName);
  const firstPrompt = cleanTitle(signal.firstPrompt) ?? (newIdentity ? undefined : current.firstPrompt);
  const submitPrompt = submitEvent ? cleanTitle(signal.prompt) ?? (newIdentity ? undefined : current.submitPrompt) : (newIdentity ? undefined : current.submitPrompt);
  const identity = sessionId && signal.at >= (current.identityAt ?? 0) ? { sessionId, sessionIdPrefix: undefined, identityAt: signal.at, sessionName, firstPrompt, submitPrompt } : { sessionName, firstPrompt, submitPrompt };
  if (signal.at < current.at) return { ...current, hookSeen: true, ...identity };
  if (signal.kind === "antigravity" && current.antigravityCompleted && !newIdentity && raw.event !== "PreInvocation" && signal.event !== "Stop") return current;
  const state = hookState(raw);
  return { ...current, hookSeen: true, ...identity,
    ...(signal.kind === "antigravity" ? { antigravityCompleted: signal.event === "Stop" } : {}),
    replyPreview: state === "working" ? undefined : cleanTitle(signal.replyPreview) ?? (newIdentity ? undefined : current.replyPreview),
    ...(!state && signal.event === "SessionStart" && current.state === "starting" ? { state: "attention" as const } : {}),
    ...(state ? { state, at: signal.at, source: "hook" as const } : {}),
  };
}

export function observeTitle(current: ProbeState, state: HarnessState, at: number, hooksAuthoritative = false): ProbeState {
  if (hooksAuthoritative && current.hookSeen) return { ...current, titleSeen: true, titleState: state };
  // A repeated spinner must not override a newer approval hook with an old working title.
  if (current.titleState === state) return { ...current, titleSeen: true };
  return { ...current, titleSeen: true, titleState: state, state, at, source: "title" };
}
