"use client";

import { persistentStorage } from "../lib/persistent-storage.ts";

import { useCallback, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";
import { isDarkTheme, isThemePreference, type ThemePreference, type ResolvedTheme } from "@/lib/theme";

export type { ThemePreference, ResolvedTheme } from "@/lib/theme";

type ThemeState = {
  preference: ThemePreference;
  theme: ResolvedTheme;
};

const STORAGE_KEY = "pi-theme";
const SERVER_SNAPSHOT: ThemeState = { preference: "auto", theme: "light" };

const listeners = new Set<() => void>();
let state: ThemeState | null = null;
let systemListening = false;
let activeTransition: ViewTransition | null = null;

function emit(): void {
  listeners.forEach((cb) => cb());
}

function getSystemTheme(): ResolvedTheme {
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function readStoredPreference(): ThemePreference {
  try {
    const value = persistentStorage().getItem(STORAGE_KEY);
    if (isThemePreference(value)) return value;
  } catch {
    // ignore storage errors (private mode, quota, etc.)
  }
  return "auto";
}

function resolveTheme(preference: ThemePreference): ResolvedTheme {
  return preference === "auto" ? getSystemTheme() : preference;
}

function applyDomTheme(theme: ResolvedTheme): void {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = theme;
  document.documentElement.classList.toggle("dark", isDarkTheme(theme));
}

function ensureState(): ThemeState {
  if (typeof window === "undefined") return SERVER_SNAPSHOT;
  if (state) return state;

  const preference = readStoredPreference();
  const theme = resolveTheme(preference);
  applyDomTheme(theme);
  state = { preference, theme };
  return state;
}

function setThemeState(preference: ThemePreference, theme: ResolvedTheme, persist: boolean): void {
  applyDomTheme(theme);
  if (persist) {
    try {
      persistentStorage().setItem(STORAGE_KEY, preference);
    } catch {
      // ignore storage errors (private mode, quota, etc.)
    }
  }
  state = { preference, theme };
  emit();
}

function syncAutoThemeFromSystem(): void {
  const current = ensureState();
  if (current.preference !== "auto") return;
  const theme = getSystemTheme();
  if (theme === current.theme) return;
  setThemeState("auto", theme, false);
}

function syncThemeFromStorage(event: StorageEvent): void {
  if (event.key !== null && event.key !== STORAGE_KEY) return;
  const preference = readStoredPreference();
  const theme = resolveTheme(preference);
  const current = ensureState();
  if (current.preference === preference && current.theme === theme) return;
  setThemeState(preference, theme, false);
}

function ensureThemeListeners(): void {
  if (systemListening || typeof window === "undefined" || !window.matchMedia) return;

  const mql = window.matchMedia("(prefers-color-scheme: dark)");
  mql.addEventListener("change", syncAutoThemeFromSystem);
  window.addEventListener("storage", syncThemeFromStorage);
  // Some browsers delay or miss scheme events while backgrounded.
  window.addEventListener("focus", syncAutoThemeFromSystem);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") syncAutoThemeFromSystem();
  });
  systemListening = true;
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  ensureState();
  ensureThemeListeners();
  syncAutoThemeFromSystem();
  return () => {
    listeners.delete(cb);
  };
}

function getSnapshot(): ThemeState {
  return ensureState();
}

function getServerSnapshot(): ThemeState {
  return SERVER_SNAPSHOT;
}

export function useTheme() {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);

  const setThemePreference = useCallback((nextPreference: ThemePreference) => {
    const current = ensureState();
    if (current.preference === nextPreference) return;
    const nextTheme = resolveTheme(nextPreference);
    activeTransition?.skipTransition();
    activeTransition = null;
    const apply = () => setThemeState(nextPreference, nextTheme, true);
    if (nextTheme === current.theme || typeof document.startViewTransition !== "function"
      || window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      apply();
      return;
    }
    const x = window.innerWidth / 2;
    const y = window.innerHeight / 2;
    const radius = Math.ceil(Math.hypot(x, y));
    const transition = document.startViewTransition(() => flushSync(apply));
    activeTransition = transition;
    void transition.ready.then(() => {
      if (activeTransition !== transition) return;
      document.documentElement.animate({
        clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`],
      }, {
        duration: 220,
        easing: "linear",
        fill: "forwards",
        pseudoElement: "::view-transition-new(root)",
      });
    }).catch(() => { /* A newer choice can cancel snapshot capture. */ });
    void transition.finished.finally(() => {
      if (activeTransition === transition) activeTransition = null;
    }).catch(() => {});
  }, []);

  return {
    theme: snapshot.theme,
    preference: snapshot.preference,
    setThemePreference,
    isDark: isDarkTheme(snapshot.theme),
  };
}
