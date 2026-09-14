"use client";

import { useEffect, useState } from "react";
import type { CardQueue } from "@/lib/card-queue";
import { nextQueueScoreChange } from "@/lib/card-queue-snapshot";

/** Waiting scores advance independently of unchanged network snapshots. */
export function useQueueScoreClock(queue: CardQueue | null) {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!queue || queue.sortMode !== "score") return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const advance = () => setTick(value => value + 1);
    const onVisibility = () => {
      clearTimeout(timer);
      if (document.visibilityState === "visible") advance();
    };
    if (document.visibilityState === "visible") {
      const now = Date.now();
      const next = nextQueueScoreChange(queue, now);
      if (next !== null) timer = setTimeout(advance, Math.min(2_147_483_647, Math.max(1, next - now)));
    }
    document.addEventListener("visibilitychange", onVisibility);
    return () => { clearTimeout(timer); document.removeEventListener("visibilitychange", onVisibility); };
  }, [queue, tick]);
  return tick;
}
