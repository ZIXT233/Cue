"use client";

import { persistentStorage } from "../lib/persistent-storage.ts";
import { useCallback, useEffect, useState } from "react";
import {
  ATTENTION_MODE_CHANGED,
  ATTENTION_MODE_KEY,
  attentionModeFromStorage,
  type AttentionMode,
} from "@/lib/attention-mode";

export function useAttentionMode() {
  // Always start with the SSR-safe default so server/client markup matches.
  // localStorage is applied after mount.
  const [mode, setModeState] = useState<AttentionMode>("daily");

  useEffect(() => {
    const sync = () => {
      setModeState(attentionModeFromStorage(persistentStorage().getItem(ATTENTION_MODE_KEY)));
    };
    sync();
    window.addEventListener("storage", sync);
    window.addEventListener(ATTENTION_MODE_CHANGED, sync);
    return () => {
      window.removeEventListener("storage", sync);
      window.removeEventListener(ATTENTION_MODE_CHANGED, sync);
    };
  }, []);

  const setMode = useCallback((next: AttentionMode) => {
    persistentStorage().setItem(ATTENTION_MODE_KEY, next);
    setModeState(next);
    window.dispatchEvent(new Event(ATTENTION_MODE_CHANGED));
  }, []);

  return { mode, setMode };
}
