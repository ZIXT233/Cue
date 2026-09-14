import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
export interface TerminalTranscript { cwd: string; output: string; exitCode: number | null }
const directory = () => join(process.env.TOPCARD_DATA_DIR || join(process.cwd(), ".topcard"), "terminal-transcripts");
export function saveTerminalTranscript(id: string, transcript: TerminalTranscript): void {
  if (!/^[a-f0-9]{32}$/.test(id)) return;
  const file = join(directory(), `${id}.json`);
  try {
    mkdirSync(directory(), { recursive: true, mode: 0o700 });
    writeFileSync(`${file}.tmp`, JSON.stringify(transcript), { mode: 0o600 });
    renameSync(`${file}.tmp`, file);
  } catch { /* Persistence failure must not stop the terminal. */ }
}
export function readTerminalTranscript(id: string): TerminalTranscript | null {
  if (!/^[a-f0-9]{32}$/.test(id)) return null;
  try {
    const saved = JSON.parse(readFileSync(join(directory(), `${id}.json`), "utf8"));
    return typeof saved.cwd === "string" && typeof saved.output === "string" ? saved : null;
  } catch { return null; }
}
