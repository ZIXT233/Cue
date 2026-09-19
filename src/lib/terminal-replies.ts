const DA1 = /^\x1b\[\?[\d;]*c$/;
const REPLY = /^\x1b\[(?:[?>]?[\d;]*[cnR]|[IO])$/;
const HOST_PREFIX = "\x1b[1t\x1b[c";

/** Protocol replies must not wait for a rendering frame or user-input work. */
export function isTerminalProtocolReply(data: string): boolean {
  return REPLY.test(data) || /^\x1b\](?:4;\d+|10|11|12);rgb:[\da-f]+\/[\da-f]+\/[\da-f]+(?:\x07|\x1b\\)$/i.test(data);
}

/** Suppress historical replies and only the host handshake already answered
 * by the native reader. Live application probes use normal terminal behavior. */
export class TerminalReplyPolicy {
  private prefix = "";
  private looking: boolean;
  private hostDa1Pending = false;
  constructor(modernConpty: boolean) { this.looking = modernConpty; }
  observeOutput(data: string): void {
    if (!this.looking) return;
    for (const char of data) {
      this.prefix += char;
      if (!HOST_PREFIX.startsWith(this.prefix)) { this.looking = false; return; }
      if (this.prefix === HOST_PREFIX) { this.hostDa1Pending = true; this.looking = false; return; }
    }
  }
  suppress(data: string, replaying: boolean, ownsFocus: boolean): boolean {
    if (this.hostDa1Pending && DA1.test(data)) { this.hostDa1Pending = false; return true; }
    if (ownsFocus && (data === "\x1b[I" || data === "\x1b[O")) return true;
    return replaying && REPLY.test(data);
  }
}
