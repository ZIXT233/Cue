import { harnessName } from "./catalog.ts";
import type { HarnessSession } from "./types.ts";

export function cardTitle(
  harness: HarnessSession | undefined,
  workspaceName: string | undefined,
  fallback: string,
) {
  if (!harness) return fallback;
  return harness.sessionName
    || harness.firstPrompt
    || harness.submitPrompt
    || `${harnessName(harness.kind)} · ${workspaceName || fallback}`;
}
