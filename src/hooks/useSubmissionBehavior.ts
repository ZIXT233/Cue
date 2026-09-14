"use client";

import { persistentStorage } from "../lib/persistent-storage.ts";
import { useCallback, useEffect, useState } from "react";
import {
  SUBMISSION_BEHAVIOR_CHANGED,
  SUBMISSION_BEHAVIOR_KEY,
  submissionBehaviorFromStorage,
  type SubmissionBehavior,
} from "@/lib/submission-behavior";

export function useSubmissionBehavior() {
  // Always start with the SSR-safe default so server/client markup matches.
  // localStorage is applied after mount.
  const [mode, setModeState] = useState<SubmissionBehavior>("keep-in-view");

  useEffect(() => {
    const sync = () => {
      setModeState(submissionBehaviorFromStorage(persistentStorage().getItem(SUBMISSION_BEHAVIOR_KEY)));
    };
    sync();
    window.addEventListener("storage", sync);
    window.addEventListener(SUBMISSION_BEHAVIOR_CHANGED, sync);
    return () => {
      window.removeEventListener("storage", sync);
      window.removeEventListener(SUBMISSION_BEHAVIOR_CHANGED, sync);
    };
  }, []);

  const setMode = useCallback((next: SubmissionBehavior) => {
    persistentStorage().setItem(SUBMISSION_BEHAVIOR_KEY, next);
    setModeState(next);
    window.dispatchEvent(new Event(SUBMISSION_BEHAVIOR_CHANGED));
  }, []);

  return { mode, setMode };
}
