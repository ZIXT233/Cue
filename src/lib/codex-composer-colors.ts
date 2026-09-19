import type { ITheme } from "@xterm/xterm";
import { paletteTerminalTheme } from "./terminal-theme";
import { TERMINAL_PALETTES } from "./terminal-appearance";

// Codex 0.155 caches OSC 11 at startup and emits the composer tint as RGB.
// Bind only those known tints to a theme palette slot so existing cells can
// change color without replaying output or writing anything into the CLI.
// Formula: openai/codex rust-v0.155.0, tui/src/style.rs::user_message_bg_rgb.
function tint(background: string, dark: boolean): number[] {
  return [1, 3, 5].map(i => Math.floor(parseInt(background.slice(i, i + 2), 16) * (dark ? .88 : .96) + (dark ? 255 * .12 : 0)));
}
const composerColors = new Set([
  "41;41;41", // Legacy Win32 default #0c0c0c.
  ...TERMINAL_PALETTES.flatMap(p => [false, true].map(d => tint(paletteTerminalTheme(d, p).background!, d).join(";"))),
]);

export function codexComposerTheme(theme: ITheme, dark: boolean): ITheme {
  const extendedAnsi = Array.from({ length: 240 }, (_, n) => {
    const i = n + 16;
    const rgb = i < 232
      ? [Math.floor((i - 16) / 36), Math.floor((i - 16) / 6) % 6, (i - 16) % 6].map(v => v === 0 ? 0 : 55 + v * 40)
      : [8 + (i - 232) * 10, 8 + (i - 232) * 10, 8 + (i - 232) * 10];
    return `#${rgb.map(v => v.toString(16).padStart(2, "0")).join("")}`;
  });
  extendedAnsi[239] = `#${tint(theme.background!, dark).map(v => v.toString(16).padStart(2, "0")).join("")}`;
  return { ...theme, extendedAnsi };
}

function rewriteSgr(sequence: string, matchesComposer: (rgb: string) => boolean): string {
  if (!/^\x1b\[[\d;]*m$/.test(sequence)) return sequence;
  const values = sequence.slice(2, -1).split(";");
  const out: string[] = [];
  for (let i = 0; i < values.length; i++) {
    const code = values[i];
    if ((code === "38" || code === "48" || code === "58") && values[i + 1] === "2" && i + 4 < values.length) {
      const rgb = values.slice(i + 2, i + 5);
      // Only backgrounds are eligible. Matching foreground RGB is ordinary
      // content too; rebinding it to the background can make text disappear.
      if (code === "48" && matchesComposer(rgb.join(";"))) out.push(code, "5", "255");
      else out.push(code, "2", ...rgb);
      i += 4;
    } else if ((code === "38" || code === "48" || code === "58") && values[i + 1] === "5" && i + 2 < values.length) {
      // Preserve original color 255; only our semantic tint owns the slot.
      if (values[i + 2] === "255") out.push(code, "2", "238", "238", "238");
      else out.push(code, "5", values[i + 2]);
      i += 2;
    } else out.push(code);
  }
  return `\x1b[${out.join(";")}m`;
}

/** Incremental VT filter: never interpret CSI-like text inside OSC/DCS strings. */
export class CodexComposerColors {
  private state: "text" | "escape" | "csi" | "string" | "stringEscape" = "text";
  private pending = "";
  private stringIsOsc = false;
  private boundTint: string | undefined;
  reset(): void { this.state = "text"; this.pending = ""; this.stringIsOsc = false; }
  private matchesComposer = (rgb: string): boolean => {
    // A running Codex caches one startup palette. Once identified, do not
    // reinterpret every other preset's tint elsewhere in its output.
    if (this.boundTint === undefined && composerColors.has(rgb)) this.boundTint = rgb;
    return rgb === this.boundTint;
  };
  feed(data: string): string {
    let output = "";
    for (const c of data) {
      if (this.state === "string" || this.state === "stringEscape") {
        output += c;
        if ((this.stringIsOsc && c === "\x07") || c === "\x18" || c === "\x1a" || (this.state === "stringEscape" && c === "\\")) this.state = "text";
        else this.state = c === "\x1b" ? "stringEscape" : "string";
      } else if (this.state === "csi") {
        this.pending += c;
        if (c >= "@" && c <= "~") {
          output += c === "m" ? rewriteSgr(this.pending, this.matchesComposer) : this.pending;
          this.pending = ""; this.state = "text";
        } else if (this.pending.length > 1024 || c === "\x1b") {
          output += this.pending; this.pending = ""; this.state = "text";
        }
      } else if (this.state === "escape") {
        if (c === "[") { this.pending = "\x1b["; this.state = "csi"; }
        else { output += `\x1b${c}`; this.stringIsOsc = c === "]"; this.state = "]PX^_".includes(c) ? "string" : "text"; }
      } else if (c === "\x1b") this.state = "escape";
      else output += c;
    }
    return output;
  }
}
