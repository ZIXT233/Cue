import { effectiveTurnTags, DEFAULT_TURN_TAGS } from "./turn-priority.ts";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { EMPTY_QUEUE, type CardQueue } from "./card-queue.ts";
import { notifyQueueChanged } from "./card-queue-live.ts";

const queueFile = () => process.env.TOPCARD_QUEUE_FILE || join(process.env.TOPCARD_DATA_DIR || join(process.cwd(), ".topcard"), "queue.json");
const globalQueue = globalThis as typeof globalThis & { __cardQueueLock?: Promise<unknown> };

// Writes replace the file atomically. A bootstrap reader can safely read the
// last committed board without waiting for live reconciliation or mutations.
export async function readCardQueueSnapshot(): Promise<CardQueue> {
  try {
    const state = JSON.parse(await readFile(queueFile(), "utf8")) as CardQueue;
    if (state.version !== 1 || !Array.isArray(state.cards) || !Array.isArray(state.order)) throw new Error("Invalid card queue file");
    state.turnTagDefinitions = effectiveTurnTags(state.turnTagDefinitions ?? DEFAULT_TURN_TAGS);
    return state;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    return structuredClone(EMPTY_QUEUE);
  }
}

// One Node process owns the Pi runtime. Serialize both reads/reconciliation and
// mutations across tabs and HMR, and commit with an atomic rename.
export function withCardQueue<T>(action: (state: CardQueue) => Promise<T> | T, options?: { silent?: boolean }): Promise<T> {
  const task = (globalQueue.__cardQueueLock ?? Promise.resolve()).catch(() => {}).then(async () => {
    let state: CardQueue;
    try {
      state = JSON.parse(await readFile(queueFile(), "utf8"));
      if (state.version !== 1 || !Array.isArray(state.cards) || !Array.isArray(state.order)) throw new Error("Invalid card queue file");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      state = structuredClone(EMPTY_QUEUE);
    }
    const before = JSON.stringify(state);
    const desktopInstance = process.env.TOPCARD_DESKTOP_INSTANCE;
    if (desktopInstance) {
      for (const card of state.cards) {
        if (card.detached && !card.detached.owner.startsWith(`desktop:${desktopInstance}:`)) delete card.detached;
      }
    }
    state.turnTagDefinitions = effectiveTurnTags(state.turnTagDefinitions ?? DEFAULT_TURN_TAGS);
    const result = await action(state);
    if (JSON.stringify(state) !== before) {
      state.revision++;
      const path = queueFile();
      await mkdir(dirname(path), { recursive: true });
      const temporary = `${path}.${process.pid}.tmp`;
      await writeFile(temporary, JSON.stringify(state, null, 2), { mode: 0o600 });
      await rename(temporary, path);
      if (!options?.silent) notifyQueueChanged("queue");
    }
    return result;
  });
  globalQueue.__cardQueueLock = task;
  return task;
}
