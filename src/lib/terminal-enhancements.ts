import type { TerminalKeyEventLike } from "./terminal-input";

/** Common CSI-u combinations supported by agent TUIs; not full Kitty negotiation. */
export function enhancedTerminalKey(event: TerminalKeyEventLike & { isComposing?: boolean }, mac: boolean): string | null {
  if (event.isComposing) return null;
  if (mac && event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
    if (event.key === "Backspace") return "\x15";
    if (event.key === "Delete") return "\x0b";
    if (event.key === "ArrowLeft") return "\x01";
    if (event.key === "ArrowRight") return "\x05";
  }
  if (mac && event.altKey && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
    if (event.key === "ArrowLeft") return "\x1bb";
    if (event.key === "ArrowRight") return "\x1bf";
    if (event.key === "Backspace") return "\x1b\x7f";
  }
  if (event.key === "Enter" && !event.metaKey && !event.altKey && (event.shiftKey || event.ctrlKey)) {
    return `\x1b[13;${1 + (event.shiftKey ? 1 : 0) + (event.ctrlKey ? 4 : 0)}u`;
  }
  return null;
}

/** Only clipboard writes: never answer a remote clipboard read/query. */
export function decodeTerminalClipboard(payload: string): string | null {
  const separator = payload.indexOf(";");
  if (separator < 0 || !/^[cps0-7]*$/.test(payload.slice(0, separator))) return null;
  const data = payload.slice(separator + 1);
  if (data === "?" || data.length > 1_400_000 || !/^[A-Za-z0-9+/]*={0,2}$/.test(data)) return null;
  try { return new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(atob(data), (char) => char.charCodeAt(0))); }
  catch { return null; }
}
