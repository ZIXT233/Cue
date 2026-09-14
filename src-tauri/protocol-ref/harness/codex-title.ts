import { readFile, stat, readdir } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

// Codex's title index contains ids/names only. We never load conversation history.
const cache = globalThis as typeof globalThis & { __topcardCodexTitles?: { path: string; stamp: string; titles: Map<string, string> } };
export async function codexSessionTitle(sessionId: string): Promise<string | undefined> {
  const path = join(process.env.CODEX_HOME || join(homedir(), ".codex"), "session_index.jsonl");
  try {
    const info = await stat(path);
    const stamp = `${info.mtimeMs}:${info.size}`;
    if (cache.__topcardCodexTitles?.path !== path || cache.__topcardCodexTitles.stamp !== stamp) {
      const titles = new Map<string, string>();
      for (const line of (await readFile(path, "utf8")).split("\n")) {
        try {
          const entry = JSON.parse(line);
          if (typeof entry.id === "string" && typeof entry.thread_name === "string" && entry.thread_name.trim()) titles.set(entry.id, entry.thread_name.trim());
        } catch { /* The final appended line may still be incomplete. */ }
      }
      cache.__topcardCodexTitles = { path, stamp, titles };
    }
    return cache.__topcardCodexTitles.titles.get(sessionId);
  } catch { return undefined; }
}

// Codex currently truncates each title item to 32 characters, including "...".
// Resolve only a unique on-disk identity; never use a partial id with resume.
export async function resolveCodexSessionPrefix(prefix: string): Promise<string | undefined> {
  if (!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{5}$/i.test(prefix)) return undefined;
  await codexSessionTitle("");
  const matches = new Set([...cache.__topcardCodexTitles?.titles.keys() ?? []].filter(id => id.startsWith(prefix)));
  const home = process.env.CODEX_HOME || join(homedir(), ".codex");
  for (const directory of ["sessions", "archived_sessions"]) {
    try {
      for (const file of await readdir(join(home, directory), { recursive: true })) {
        const id = file.match(/([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\.jsonl$/i)?.[1];
        if (id?.startsWith(prefix)) matches.add(id);
      }
    } catch { /* A not-yet-persisted session cannot be resolved safely. */ }
  }
  return matches.size === 1 ? [...matches][0] : undefined;
}

export async function codexSessionExists(id: string): Promise<boolean | undefined> {
  if (!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(id)) return undefined;
  const home = process.env.CODEX_HOME || join(homedir(), ".codex");
  for (const directory of ["sessions", "archived_sessions"]) {
    try {
      if ((await readdir(join(home, directory), { recursive: true })).some(file => file.endsWith(`_${id}.jsonl`))) return true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") return undefined;
    }
  }
  return false;
}
