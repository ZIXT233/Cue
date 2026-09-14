export type QueueArrivalSide = "left" | "right";

export function queueArrivalSide(
  orderedCardIds: readonly string[],
  focusedCardId: string | null | undefined,
  arrivingCardId: string,
): QueueArrivalSide | null {
  if (!focusedCardId || focusedCardId === arrivingCardId) return null;
  const focusedIndex = orderedCardIds.indexOf(focusedCardId);
  const arrivingIndex = orderedCardIds.indexOf(arrivingCardId);
  if (focusedIndex < 0 || arrivingIndex < 0) return null;
  return arrivingIndex < focusedIndex ? "left" : "right";
}

export function latestAssistantReply(messages: readonly unknown[], maxLength = 240): string {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (!message || typeof message !== "object") continue;
    const candidate = message as { role?: unknown; content?: unknown };
    if (candidate.role === "user") break;
    if (candidate.role !== "assistant") continue;
    const content = typeof candidate.content === "string"
      ? candidate.content
      : Array.isArray(candidate.content)
        ? candidate.content.flatMap((block) => {
            if (!block || typeof block !== "object") return [];
            const item = block as { type?: unknown; text?: unknown };
            return item.type === "text" && typeof item.text === "string" ? [item.text] : [];
          }).join(" ")
        : "";
    const plainText = content
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/[`*_>#]/g, "")
      .replace(/[\x00-\x1f\x7f]/g, " ")
      .replace(/\s+/g, " ")
      .trim();
    if (plainText) return plainText.slice(0, maxLength);
  }
  return "";
}
