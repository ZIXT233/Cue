import { harnessName } from "./catalog.ts";
import type { HarnessSession } from "./types.ts";

export function cardTitle(
  harness: HarnessSession | undefined,
  workspaceName: string | undefined,
  fallback: string,
) {
  if (!harness) return fallback;
  // Explicit/session names stay stable; unnamed sessions follow the latest user turn.
  return harness.sessionName?.trim()
    || harness.submitPrompt?.trim()
    || harness.firstPrompt?.trim()
    || `${harnessName(harness.kind)} · ${workspaceName || fallback}`;
}
