import type { QueueCard } from "./card-queue.ts";

// Initial snapshots and metadata-only updates must never replay old completions.
export function completedCards(previous: QueueCard[] | null, cards: QueueCard[]): QueueCard[] {
  if (!previous) return [];
  const before = new Map(previous.map(card => [card.id, card]));
  return cards.filter(card => {
    const old = before.get(card.id);
    return old?.phase === "working" && card.phase === "attention"
      && (card.harness?.kind !== "shell" || !!old.harness?.shellNotify)
      && (!card.harness || card.harness.state === "attention")
      && card.archivedAt === undefined && !!(card.session || card.harness)
      && card.session?.relation?.kind !== "subagent";
  });
}
