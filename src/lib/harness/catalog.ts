import type { HarnessId } from "./types";

export const harnessCatalog: { id: HarnessId; name: string; description: string; hidden?: boolean }[] = [
  { id: "codex", name: "Codex", description: "OpenAI · CLI" },
  { id: "claude", name: "Claude Code", description: "Anthropic · CLI" },
  { id: "cursor", name: "Cursor Agent", description: "Cursor · CLI" },
  { id: "pi", name: "Pi CLI", description: "Pi · CLI" },
  { id: "omp", name: "Oh My Pi", description: "OMP · CLI" },
  { id: "grok", name: "Grok Build", description: "xAI · CLI" },
  { id: "antigravity", name: "Antigravity CLI", description: "Google · CLI", hidden: true },
  { id: "opencode", name: "OpenCode", description: "OpenCode · CLI" },
  { id: "shell", name: "Shell", description: "手动放入工作区，命令完成通知；不接入 Harness 通知探针" },
];
export const harnessPicker = harnessCatalog.filter(item => !item.hidden);
export const harnessName = (id: HarnessId) => harnessCatalog.find(item => item.id === id)?.name ?? id;
