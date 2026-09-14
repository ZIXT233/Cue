import { antigravityAdapter, claudeAdapter, cursorAdapter, piAdapter, ompAdapter, grokAdapter, geminiAdapter, opencodeAdapter, shellAdapter } from "./native";
import { codexAdapter } from "./codex.ts";
import type { HarnessAdapter } from "./types.ts";

// Each future CLI supplies launch arguments and its own lightweight state probe.
export function getHarnessAdapter(id: unknown): HarnessAdapter {
  if (id === "codex") return codexAdapter;
  if (id === "claude") return claudeAdapter;
  if (id === "cursor") return cursorAdapter;
  if (id === "pi") return piAdapter;
  if (id === "omp") return ompAdapter;
  if (id === "grok") return grokAdapter;
  if (id === "gemini") return geminiAdapter;
  if (id === "opencode") return opencodeAdapter;
  if (id === "antigravity") return antigravityAdapter;
  if (id === "shell") return shellAdapter;
  throw new Error("不支持的 CLI agent");
}
