import { persistentStorage } from "./persistent-storage";
import { readTerminalBackground, resolveTerminalDark, syncTerminalCanvasToBackend } from "./terminal-background";

export const TERMINAL_PALETTES = ["solarized", "catppuccin", "github", "gruvbox"] as const;
export type TerminalPalette = typeof TERMINAL_PALETTES[number];
export const TERMINAL_PALETTE_LABELS: Record<TerminalPalette, string> = {
  solarized: "Solarized", catppuccin: "Catppuccin", github: "GitHub", gruvbox: "Gruvbox",
};
export const TERMINAL_FONTS = ["default", "Cascadia Mono", "Consolas", "JetBrains Mono", "Fira Code", "Menlo", "DejaVu Sans Mono"] as const;
export interface TerminalAppearance { light: TerminalPalette; dark: TerminalPalette; font: string; codexAdaptiveBackground: boolean }
export const TERMINAL_APPEARANCE_KEY = "que-terminal-appearance";
export const TERMINAL_APPEARANCE_EVENT = "que:terminal-appearance";
const defaults: TerminalAppearance = { light: "solarized", dark: "solarized", font: "default", codexAdaptiveBackground: true };

export function normalizeTerminalAppearance(value: Partial<TerminalAppearance> | null): TerminalAppearance {
  const palette = (v: unknown): TerminalPalette => TERMINAL_PALETTES.includes(v as TerminalPalette) ? v as TerminalPalette : "solarized";
  return { light: palette(value?.light), dark: palette(value?.dark), codexAdaptiveBackground: value?.codexAdaptiveBackground !== false,
    font: typeof value?.font === "string" && TERMINAL_FONTS.includes(value.font as typeof TERMINAL_FONTS[number]) ? value.font : "default" };
}
export function readTerminalAppearance(): TerminalAppearance {
  try { return normalizeTerminalAppearance(JSON.parse(persistentStorage().getItem(TERMINAL_APPEARANCE_KEY) ?? "null")); }
  catch { return { ...defaults }; }
}
export function saveTerminalAppearance(value: TerminalAppearance): void {
  const next = normalizeTerminalAppearance(value);
  const previous = readTerminalAppearance();
  try { persistentStorage().setItem(TERMINAL_APPEARANCE_KEY, JSON.stringify(next)); } catch { /* storage unavailable */ }
  window.dispatchEvent(new CustomEvent(TERMINAL_APPEARANCE_EVENT, { detail: next }));
  // Update xterm palettes synchronously above, then ask subscribed CLIs to
  // query again, including switches between palettes of the same brightness.
  const dark = resolveTerminalDark(document.documentElement.classList.contains("dark"), readTerminalBackground());
  const mode = dark ? "dark" : "light";
  if (previous[mode] !== next[mode]) syncTerminalCanvasToBackend(dark, true);
}
export function terminalFontFamily(font: string, fallback: string): string {
  return font === "default" ? fallback : `"${font}", ${fallback}`;
}

// Preserve the existing shared chat/terminal scale; preview and xterm must agree.
export function terminalFontSize(chatSize: number): number {
  return Math.max(9, Math.min(22, Math.round((Number.isFinite(chatSize) ? chatSize : 14) - 1)));
}
