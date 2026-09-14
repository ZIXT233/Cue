export const DECK_CARD_GAP = 24;

export function deckStep(width: number): number {
  return Math.max(320, width * .85);
}

export function projectDeckCard(index: number, position: number, width: number) {
  const distance = index - position;
  const depth = Math.abs(distance);
  const scale = 1 - Math.min(5, depth) * .035;
  const previewStride = Math.max(120, width * .18);
  const travel = Math.min(1, Math.max(0, -distance));
  const blend = travel * travel * (3 - 2 * travel);
  const stride = previewStride + (width + DECK_CARD_GAP - previewStride) * blend;
  return {
    distance,
    // Open out the stack continuously as a card crosses the center.
    x: distance < 0 ? distance * stride + width * (1 - scale) : distance * previewStride,
    scale,
  };
}

export function deckSnapTarget(scrollLeft: number, step: number, velocity = 0, maxScroll = Infinity): number {
  // A short flick may advance one card; a slow drag docks by position.
  const projectedTravel = Math.abs(velocity) < 350 ? 0 : Math.max(-step * .65, Math.min(step * .65, velocity * .12));
  return Math.max(0, Math.min(maxScroll, Math.round((scrollLeft + projectedTravel) / step) * step));
}

/** Keep the full card, including its shadow, until it has crossed the clip edge. */
export function isDeckCardOffscreenLeft(x: number, cardWidth: number, leftBleed: number): boolean {
  return x + cardWidth + 64 < -leftBleed;
}

export type DeckHitRect = { left: number; right: number; top: number; bottom: number };

export function pointInDeckRect(x: number, y: number, rect: DeckHitRect): boolean {
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

/** Clicking an immediate neighbor preview selects it; farther previews do not. */
export function peekDeckIndexAtPoint(
  x: number,
  y: number,
  selected: number,
  front: DeckHitRect | null,
  layers: (DeckHitRect & { index: number; z: number })[],
): number | null {
  if (front && pointInDeckRect(x, y, front)) return null;
  const hits = layers.filter((layer) => (
    Number.isInteger(layer.index)
    && Math.abs(layer.index - selected) === 1
    && pointInDeckRect(x, y, layer)
  ));
  hits.sort((a, b) => b.z - a.z);
  return hits[0]?.index ?? null;
}
