export type TerminalEvent =
  // `dropped` is only present when the server had to replay from a point later
  // than our cursor: that many bytes are gone and cannot be recovered. Absent
  // means either a lossless resume or a first attach — see `reset`.
  | { type: "output"; data: string; from?: number; offset: number; reset?: boolean; dropped?: number }
  | { type: "exit"; exitCode: number }
  | { type: "closed" };

export const TERMINAL_RECONNECT_MS = 120_000;
