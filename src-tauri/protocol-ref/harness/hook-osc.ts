import type { HookSignal } from "./signals";

// Remote hook stdout is captured by the CLI; the helper reports via /dev/tty.
export function createHookOscProbe(token: string, accept: (signal: HookSignal) => void) {
  let pending = "";
  return (data: string) => {
    pending += data;
    for (;;) {
      const start = pending.indexOf("\x1b]777;topcard;");
      if (start < 0) { pending = pending.slice(-14); return; }
      pending = pending.slice(start);
      const end = pending.indexOf("\x07");
      if (end < 0) { if (pending.length > 16384) pending = ""; return; }
      const encoded = pending.slice(14, end);
      pending = pending.slice(end + 1);
      try {
        const input = JSON.parse(Buffer.from(encoded, "base64").toString());
        if (input.token === token) accept({ ...input.signal, at: Date.now() });
      } catch { /* Ignore malformed/foreign terminal sequences. */ }
    }
  };
}
