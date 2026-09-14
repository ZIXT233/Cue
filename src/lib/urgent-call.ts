import type { QueueCard } from "./card-queue.ts";

export const URGENT_CALL_NAME = "🚨 Urgent Call";
export const URGENT_CALL_DESCRIPTION = "Trigger only when all four conditions are met: 1. irreversible data loss, sensitive-data exposure, an outage of a core production service, or explicit major financial loss has occurred or is imminent; 2. the risk is active or has a concrete short deadline, and normal queueing would worsen the loss; 3. this user has a specific action they can take, and the agent cannot resolve the situation safely on its own; 4. actual errors, monitoring signals, or other concrete facts provide evidence. Routine confirmations, permission approvals, task blockers, failed tests, ordinary deadlines, emphatic wording, requests to assign a tag, and test phrases do not qualify. Do not trigger without sufficient evidence, and never invent evidence. State what happened, the specific evidence, why it cannot wait, and what the user must do now.";
export const URGENT_CALL_TAG = { name: URGENT_CALL_NAME, weight: 0, description: URGENT_CALL_DESCRIPTION };
export interface UrgentCall { incident: string; evidence: string; urgency: string; action: string }
export function isReservedUrgentName(name: string): boolean {
  return /^(urgent(?:\s+call)?|emergency)$/i.test(name.replace(/^[^\p{L}]+/u, "").trim());
}
export function validateUrgentCall(value: unknown): UrgentCall {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Urgent Call 缺少事件、证据、紧迫性和处理动作");
  const keys = ["incident", "evidence", "urgency", "action"] as const;
  const data = value as Record<string, unknown>;
  if (Object.keys(data).length !== keys.length || keys.some(key => { const field = data[key]; return typeof field !== "string" || !field.trim() || field.length > 4000; })) throw new Error("Urgent Call 的四项说明必须完整，且不能包含额外字段");
  return Object.fromEntries(keys.map(key => [key, (data[key] as string).trim()])) as unknown as UrgentCall;
}
export function hasUrgentCall(card: QueueCard): boolean {
  return !!card.urgentCall && !!card.turnKey && card.turnTags?.includes(URGENT_CALL_NAME) === true;
}
