import { invoke } from "@tauri-apps/api/core";
import { getXtermProbe } from "./terminal-probe";

export async function saveAndOpenCardLog(cardId: string, terminalId?: string) {
  const extra = terminalId ? JSON.stringify({ xterm: getXtermProbe(terminalId) }, null, 2) : undefined;
  const response = await fetch("/api/logs/report", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ cardId, termId: terminalId, extra }),
  });
  const body = await response.json() as { path?: string; error?: string };
  if (!response.ok || !body.path) {
    throw new Error(body.error || `HTTP ${response.status}`);
  }
  await invoke("reveal_log", { path: body.path });
  return body.path;
}

export async function saveAndOpenAppLog() {
  const response = await fetch("/api/tools/settings");
  const data = await response.json() as { logPath?: string; logDir?: string };
  const path = data.logPath;
  if (!path) throw new Error("log path missing");
  await invoke("reveal_log", { path });
}
