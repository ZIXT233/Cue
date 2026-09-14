// OSC 133 is the same command lifecycle protocol used by Orca's shell wrappers.
export function createShellProbe(onCommand: (running: boolean, exitCode?: number) => void) {
  let pending = "";
  return (data: string) => {
    pending += data;
    for (;;) {
      const start = pending.indexOf("\x1b]133;");
      if (start < 0) { pending = pending.slice(-7); return; }
      pending = pending.slice(start);
      const end = /\x07|\x1b\\/.exec(pending);
      if (!end) { if (pending.length > 1024) pending = ""; return; }
      const marker = pending.slice(6, end.index);
      pending = pending.slice(end.index + end[0].length);
      if (marker === "C") onCommand(true);
      else if (/^D(?:;\d+)?$/.test(marker)) onCommand(false, Number(marker.split(";")[1] || 0));
    }
  };
}
