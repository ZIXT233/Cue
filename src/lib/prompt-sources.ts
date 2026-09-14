export const PROMPT_SOURCES_ENTRY = "cue:prompt-sources";
export interface PromptSourcesConfig {
  basePrompt: string;
  excludedContextFiles: string[];
  excludedSkills: string[];
}
export const DEFAULT_PROMPT_SOURCES: PromptSourcesConfig = {
  basePrompt: "",
  excludedContextFiles: [],
  excludedSkills: [],
};
export function validatePromptSources(value: unknown): PromptSourcesConfig {
  if (!value || typeof value !== "object") throw new Error("Invalid prompt sources");
  const data = value as Record<string, unknown>;
  if (typeof data.basePrompt !== "string" || data.basePrompt.length > 200000) throw new Error("Invalid base prompt");
  for (const key of ["excludedContextFiles", "excludedSkills"] as const) {
    if (!Array.isArray(data[key]) || data[key].length > 2000 || data[key].some((path) => typeof path !== "string" || path.length > 4096)) {
      throw new Error("Invalid source selection");
    }
  }
  return {
    basePrompt: data.basePrompt,
    excludedContextFiles: [...new Set(data.excludedContextFiles as string[])],
    excludedSkills: [...new Set(data.excludedSkills as string[])],
  };
}
