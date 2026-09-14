import { persistentStorage } from "./persistent-storage.ts";

const STORAGE_KEY = "pi-thinking-expanded";
export const THINKING_EXPANDED_EVENT = "pi-thinking-expanded-changed";

export function isThinkingExpandedByDefault(): boolean {
  if (typeof window === "undefined") return false;
  return persistentStorage().getItem(STORAGE_KEY) === "true";
}

export function setThinkingExpandedByDefault(expanded: boolean): void {
  persistentStorage().setItem(STORAGE_KEY, String(expanded));
  window.dispatchEvent(new Event(THINKING_EXPANDED_EVENT));
}
