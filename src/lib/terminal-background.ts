import { persistentStorage } from "./persistent-storage";

export const TERMINAL_BACKGROUND_OPTIONS = [
  { id: "follow", label: "settings.terminalBgFollow" },
  { id: "light", label: "settings.terminalBgLight" },
  { id: "dark", label: "settings.terminalBgDark" },
] as const;

export type TerminalBackground = (typeof TERMINAL_BACKGROUND_OPTIONS)[number]["id"];

export const TERMINAL_BACKGROUND_KEY = "que-terminal-bg";

export function isTerminalBackground(value: unknown): value is TerminalBackground {
  return TERMINAL_BACKGROUND_OPTIONS.some((option) => option.id === value);
}

export function resolveTerminalDark(appDark: boolean, background: TerminalBackground): boolean {
  if (background === "light") return false;
  if (background === "dark") return true;
  return appDark;
}

export function readTerminalBackground(): TerminalBackground {
  try {
    const value = persistentStorage().getItem(TERMINAL_BACKGROUND_KEY);
    if (isTerminalBackground(value)) return value;
  } catch {
    /* private mode, quota */
  }
  return "follow";
}

export function applyTerminalBackgroundAttr(background: TerminalBackground): void {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.terminalBg = background;
}

export function syncTerminalCanvasToBackend(dark: boolean, refresh = false): void {
  if (typeof window === "undefined") return;
  void fetch("/api/terminal-theme", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ dark, refresh }),
    keepalive: true,
  }).catch(() => {});
}

export function syncResolvedTerminalCanvas(appDark: boolean): void {
  const background = readTerminalBackground();
  applyTerminalBackgroundAttr(background);
  syncTerminalCanvasToBackend(resolveTerminalDark(appDark, background));
}
