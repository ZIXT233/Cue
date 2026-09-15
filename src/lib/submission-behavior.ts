export const SUBMISSION_BEHAVIOR_KEY = "cue:submission-behavior";
export const SUBMISSION_BEHAVIOR_CHANGED = "cue:submission-behavior-changed";

export const SUBMISSION_BEHAVIORS = [
  { id: "keep-in-view", icon: "◉", label: "settings.submissionKeepInView" },
  { id: "collapse", icon: "⇲", label: "settings.submissionCollapse" },
] as const;

export type SubmissionBehavior = (typeof SUBMISSION_BEHAVIORS)[number]["id"];

export function isSubmissionBehavior(value: unknown): value is SubmissionBehavior {
  return SUBMISSION_BEHAVIORS.some((behavior) => behavior.id === value);
}

export function submissionBehaviorFromStorage(stored: string | null): SubmissionBehavior {
  return isSubmissionBehavior(stored) ? stored : "collapse";
}
