export interface XtermProbe {
  cols: number;
  rows: number;
  sseMessages: number;
  bytesWritten: number;
  lastOffset?: number;
  lastReset?: boolean;
  status: string;
  at: number;
}

const probes = new Map<string, XtermProbe>();

export function setXtermProbe(id: string, probe: Omit<XtermProbe, "at">) {
  probes.set(id, { ...probe, at: Date.now() });
}

export function getXtermProbe(id: string): XtermProbe | undefined {
  return probes.get(id);
}
