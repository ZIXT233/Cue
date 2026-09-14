"use client";

import { persistentStorage } from "../lib/persistent-storage.ts";

import { useState, useRef, useCallback, useEffect } from "react";

const SOUND_CHANGED = "topcard:sound-changed";
let pageAudioContext: AudioContext | null = null;

function playTone(ctx: AudioContext) {
  const now = ctx.currentTime;
  // Two crisp wooden-bar strikes, low then high, with a short bright attack.
  [523.25, 783.99].forEach((note, index) => {
    const start = now + index * 0.18;
    [
      { frequency: note, volume: 0.27, decay: 0.32 },
      { frequency: note * 3.96, volume: 0.075, decay: 0.095 },
    ].forEach(({ frequency, volume, decay }) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.type = "sine";
      osc.frequency.setValueAtTime(frequency, start);
      gain.gain.setValueAtTime(0, start);
      gain.gain.linearRampToValueAtTime(volume, start + 0.002);
      gain.gain.setValueAtTime(volume, start + 0.012);
      gain.gain.exponentialRampToValueAtTime(0.0001, start + decay);
      gain.gain.linearRampToValueAtTime(0, start + decay + 0.02);
      osc.onended = () => { osc.disconnect(); gain.disconnect(); };
      osc.start(start);
      osc.stop(start + decay + 0.025);
    });
  });
}

function playRightArrivalTone(ctx: AudioContext) {
  const now = ctx.currentTime;
  // A soft upward glint: distinct from the two-strike completion chime without
  // using the descending interval and buzzy triangle wave associated with errors.
  [659.25, 880].forEach((note, index) => {
    const start = now + index * 0.085;
    const volume = index === 0 ? 0.105 : 0.075;
    const decay = index === 0 ? 0.19 : 0.23;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.type = "sine";
    osc.frequency.setValueAtTime(note, start);
    gain.gain.setValueAtTime(0, start);
    gain.gain.linearRampToValueAtTime(volume, start + 0.008);
    gain.gain.exponentialRampToValueAtTime(0.0001, start + decay);
    osc.onended = () => { osc.disconnect(); gain.disconnect(); };
    osc.start(start);
    osc.stop(start + decay + 0.015);
  });
}

export function useAudio() {
  const [enabled, setEnabled] = useState<boolean>(() => {
    if (typeof window === "undefined") return true;
    const stored = persistentStorage().getItem("topcard:sound-enabled");
    return stored === null ? true : stored === "true";
  });

  const enabledRef = useRef(enabled);
  useEffect(() => { enabledRef.current = enabled; }, [enabled]);

  useEffect(() => {
    const sync = () => {
      const stored = persistentStorage().getItem("topcard:sound-enabled");
      enabledRef.current = stored === null || stored === "true";
      setEnabled(enabledRef.current);
    };
    sync();
    window.addEventListener("storage", sync);
    window.addEventListener(SOUND_CHANGED, sync);
    return () => { window.removeEventListener("storage", sync); window.removeEventListener(SOUND_CHANGED, sync); };
  }, []);

  // Reuse a single AudioContext so it can be resumed if the browser
  // autoplay policy suspends it (contexts created outside user gestures
  // start in "suspended" state and produce no sound).
  const getCtx = useCallback((): AudioContext | null => {
    if (pageAudioContext && pageAudioContext.state !== "closed") return pageAudioContext;
    try {
      pageAudioContext = new AudioContext();
    } catch {
      return null;
    }
    return pageAudioContext;
  }, []);

  const unlockAudio = useCallback((force = false) => {
    if (!force && !enabledRef.current) return;
    const ctx = getCtx();
    if (!ctx || ctx.state === "running") return;
    ctx.resume().catch(() => {});
  }, [getCtx]);

  const toggle = useCallback(() => {
    const next = !enabledRef.current;
    if (next) unlockAudio(true);
    enabledRef.current = next;
    persistentStorage().setItem("topcard:sound-enabled", String(next));
    setEnabled(next);
    window.dispatchEvent(new Event(SOUND_CHANGED));
  }, [unlockAudio]);

  const playDone = useCallback(() => {
    if (!enabledRef.current) return;
    const ctx = getCtx();
    if (!ctx) return;
    const play = () => {
      try {
        playTone(ctx);
      } catch {
        // AudioContext not available
      }
    };
    if (ctx.state !== "running") {
      ctx.resume().then(() => { if (ctx.state === "running" && enabledRef.current) play(); }).catch(() => {});
      return;
    }
    play();
  }, [getCtx]);

  const playQueueArrivalSound = useCallback((side: "left" | "right") => {
    if (!enabledRef.current) return;
    const ctx = getCtx();
    if (!ctx) return;
    const play = () => {
      try {
        if (side === "right") playRightArrivalTone(ctx);
        else playTone(ctx);
      }
      catch { /* AudioContext not available */ }
    };
    if (ctx.state !== "running") {
      ctx.resume().then(() => { if (ctx.state === "running" && enabledRef.current) play(); }).catch(() => {});
      return;
    }
    play();
  }, [getCtx]);

  useEffect(() => {
    const recover = () => {
      if (document.visibilityState === "visible" && pageAudioContext) unlockAudio();
    };
    window.addEventListener("focus", recover);
    document.addEventListener("visibilitychange", recover);
    return () => { window.removeEventListener("focus", recover); document.removeEventListener("visibilitychange", recover); };
  }, [unlockAudio]);

  const previewSound = useCallback(async (): Promise<boolean> => {
    const ctx = getCtx();
    if (!ctx) return false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      if (ctx.state !== "running") await Promise.race([
        ctx.resume(),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("Audio blocked")), 1500); }),
      ]);
      if (ctx.state !== "running") return false;
      playTone(ctx);
      return true;
    } catch { return false; }
    finally { clearTimeout(timer); }
  }, [getCtx]);

  return { previewSound, soundEnabled: enabled, onSoundToggle: toggle, playDoneSound: playDone, playQueueArrivalSound, unlockAudio, soundEnabledRef: enabledRef };
}
