import type { SessionInfo } from "./types.ts";

export type CardPhase = "draft" | "working" | "attention";
export interface QueueWorkspace {
  id: string;
  name: string;
  kind: "local" | "ssh";
  cwd: string;
  sshHost?: string;
  runtimeCwd: string;
  defaultConversationWeight?: number;
}
export interface QueueCard {
  harness?: import("./harness/types").HarnessSession;
  promptSources?: import("./prompt-sources").PromptSourcesConfig;
  id: string;
  cwd: string;
  workspaceId?: string;
  session: SessionInfo | null;
  phase: CardPhase;
  createdAt: number;
  readyAt?: number;
  priorityWeight?: number;
  waitingSince?: number;
  turnKey?: string;
  urgentCall?: import("./urgent-call.ts").UrgentCall;
  turnTags?: string[];
  tagEvaluation?: { status: "pending" | "done" | "error"; error?: string; startedAt: number; definitions: import("./turn-priority").TurnTag[]; result?: unknown };
  tagHistory?: { turnKey: string; evaluatedAt: number; definitions: import("./turn-priority").TurnTag[]; result?: unknown; error?: string }[];
  archivedAt?: number;
  detached?: { owner: string; expiresAt: number };
}
export interface CardQueue {
  version: 1;
  revision: number;
  cards: QueueCard[];
  order: string[];
  turnTagsEnabled?: boolean;
  sortMode?: "fifo" | "score";
  turnTagDefinitions?: import("./turn-priority").TurnTag[];
  insertionPosition?: "top" | "bottom";
  workspaces?: QueueWorkspace[];
}
export const EMPTY_QUEUE: CardQueue = { version: 1, revision: 0, cards: [], order: [], sortMode: "score", turnTagsEnabled: false, insertionPosition: "bottom" };
export const TAB_LEASE_MS = 120_000;

export function reconcileQueue(state: CardQueue, running: Set<string>, attention: Set<string>, now = Date.now()): CardQueue {
  const next: CardQueue = structuredClone(state);
  for (const card of next.cards) {
    if (card.detached && card.detached.expiresAt <= now) delete card.detached;
    if (!card.session && !card.harness) continue;
    const sessionId = card.session?.id ?? card.id;
    const phase = card.harness ? (card.harness.state === "working" ? "working" : "attention") : attention.has(sessionId) ? "attention" : running.has(sessionId) ? "working" : "attention";
    if (card.archivedAt !== undefined) {
      if (phase === "working" || attention.has(sessionId)) delete card.archivedAt;
      else { next.order = next.order.filter((id) => id !== card.id); continue; }
    }
    if (phase === "working") {
      next.order = next.order.filter((id) => id !== card.id);
      delete card.readyAt; delete card.waitingSince; delete card.turnKey;
      delete card.turnTags; delete card.tagEvaluation; delete card.urgentCall;
    }
    else if (!next.order.includes(card.id)) {
      card.readyAt ??= now;
      if (next.insertionPosition === "top") next.order.unshift(card.id);
      else next.order.push(card.id);
    }
    if (phase === "attention") card.readyAt ??= now;
    card.phase = phase;
  }
  next.order = [...new Set(next.order)].filter((id) => next.cards.some((card) => card.id === id && card.phase !== "working"));
  pinDraft(next);
  return next;
}

export function moveCard(state: CardQueue, id: string, position: "front" | "back"): void {
  const card = state.cards.find((item) => item.id === id);
  if (!card || card.phase === "working") return;
  delete card.archivedAt;
  state.order = state.order.filter((item) => item !== id);
  if (position === "front") state.order.unshift(id);
  else state.order.push(id);
  pinDraft(state);
}

export function releaseCard(card: QueueCard, owner: string): void {
  // A stale pagehide must never release a newer tab's claim.
  if (card.detached?.owner === owner) delete card.detached;
}

/** Keep one unsent composer outside the attention queue. */
export function pinDraft(state: CardQueue): void {
  const drafts = state.cards.filter((card) => !card.session && !card.harness);
  const draft = drafts.reduce<QueueCard | undefined>((latest, card) => !latest || card.createdAt > latest.createdAt ? card : latest, undefined);
  if (!draft) return;
  state.cards = state.cards.filter((card) => card.session || card.harness || card.id === draft.id);
  const ids = new Set(state.cards.filter((card) => card.session || card.harness).map((card) => card.id));
  state.order = state.order.filter((id) => ids.has(id));
}

export function archiveCard(state: CardQueue, id: string, now = Date.now()): void {
  const card = state.cards.find((item) => item.id === id);
  if (!card || (!card.session && !card.harness)) throw new Error("空白卡片无需归档");
  if (card.phase === "working" || card.detached) throw new Error("请先结束工作并收回卡片");
  card.archivedAt = now;
  state.order = state.order.filter((item) => item !== id);
}

export function filterHistoricalSessions(sessions: SessionInfo[], cards: QueueCard[], query = ""): SessionInfo[] {
  const activeIds = new Set(cards.flatMap((card) => card.session && card.archivedAt === undefined ? [card.session.id] : []));
  for (const card of cards) if (card.harness?.kind === "pi" && card.harness.providerSessionId) activeIds.add(card.harness.providerSessionId);
  const search = query.trim().toLowerCase();
  return sessions.filter((session) => !activeIds.has(session.id)
    && `${session.name} ${session.firstMessage} ${session.cwd}`.toLowerCase().includes(search));
}

export function deferCard(state: CardQueue, id: string): void {
  const card = state.cards.find((item) => item.id === id);
  if (!card || (!card.session && !card.harness) || card.phase === "working" || card.archivedAt !== undefined) return;
  const index = state.order.indexOf(id);
  const next = state.order.findIndex((otherId, position) => position > index && state.cards.some((other) => other.id === otherId && !other.detached));
  if (index < 0 || next < 0) return;
  [state.order[index], state.order[next]] = [state.order[next], state.order[index]];
  pinDraft(state);
}
