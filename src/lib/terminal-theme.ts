import type { ITheme } from "@xterm/xterm";

export type TerminalThemeProfile = "grok" | "campbell";

export interface TerminalThemeHost {
  remote?: boolean;
  desktopPlatform?: string;
  platform?: string;
  userAgent?: string;
}

// Original Solarized ANSI palette, shared by the light and dark variants.
const ansi: ITheme = {
  black: "#073642", red: "#dc322f", green: "#859900", yellow: "#b58900",
  blue: "#268bd2", magenta: "#d33682", cyan: "#2aa198", white: "#eee8d5",
  brightBlack: "#002b36", brightRed: "#cb4b16", brightGreen: "#586e75", brightYellow: "#657b83",
  brightBlue: "#839496", brightMagenta: "#6c71c4", brightCyan: "#93a1a1", brightWhite: "#fdf6e3",
};

// Windows Terminal / ConPTY default. Canvas and ANSI black stay near #0C0C0C so
// CLI input boxes that ConPTY emits as true black do not sit on Solarized teal.
const campbell: ITheme = {
  background: "#0C0C0C",
  foreground: "#CCCCCC",
  cursor: "#FFFFFF",
  cursorAccent: "#0C0C0C",
  selectionBackground: "#264F78",
  selectionForeground: "#FFFFFF",
  black: "#0C0C0C", red: "#C50F1F", green: "#13A10E", yellow: "#C19C00",
  blue: "#0037DA", magenta: "#881798", cyan: "#3A96DD", white: "#CCCCCC",
  brightBlack: "#767676", brightRed: "#E74856", brightGreen: "#16C60C", brightYellow: "#F9F1A5",
  brightBlue: "#3B78FF", brightMagenta: "#B4009E", brightCyan: "#61D6D6", brightWhite: "#F2F2F2",
};

export function solarizedTerminalTheme(dark: boolean): ITheme {
  return { ...ansi,
    background: dark ? "#002b36" : "#fdf6e3",
    foreground: dark ? "#839496" : "#657b83",
    cursor: dark ? "#93a1a1" : "#586e75",
    cursorAccent: dark ? "#002b36" : "#fdf6e3",
    selectionBackground: dark ? "#073642" : "#eee8d5",
    selectionForeground: dark ? "#93a1a1" : "#586e75",
  };
}

export function campbellTerminalTheme(): ITheme {
  return { ...campbell };
}

// Native Windows ConPTY only. SSH and other platforms keep Solarized because
// their PTY does not rewrite ANSI black into rgb(0,0,0).
export function isWindowsConptyHost(host: TerminalThemeHost = {}): boolean {
  if (host.remote) return false;
  if (host.desktopPlatform) return host.desktopPlatform === "win32";
  return /^Win/i.test(host.platform ?? "") || /Windows/i.test(host.userAgent ?? "");
}

export function resolveTerminalThemeProfile(
  explicit: TerminalThemeProfile | undefined,
  host: TerminalThemeHost = {},
): TerminalThemeProfile | undefined {
  if (explicit === "grok") return "grok";
  return isWindowsConptyHost(host) ? "campbell" : explicit;
}

export function terminalThemeHostFromDocument(
  remote: boolean | undefined,
  root: Pick<HTMLElement, "dataset"> | null | undefined,
  nav: Pick<Navigator, "platform" | "userAgent"> | null | undefined,
): TerminalThemeHost {
  return {
    remote,
    desktopPlatform: root?.dataset.desktopPlatform,
    platform: nav?.platform,
    userAgent: nav?.userAgent,
  };
}

// Match Grok's own dark canvas without rewriting ANSI colors emitted by the CLI.
export function harnessTerminalTheme(dark: boolean, profile?: TerminalThemeProfile): ITheme {
  if (profile === "grok") {
    return { ...solarizedTerminalTheme(true), background: "#131313", foreground: "#d4d4d4",
      cursor: "#d4d4d4", cursorAccent: "#131313",
      selectionBackground: "#3a3a3a", selectionForeground: "#ffffff" };
  }
  if (profile === "campbell") return campbellTerminalTheme();
  return solarizedTerminalTheme(dark);
}
