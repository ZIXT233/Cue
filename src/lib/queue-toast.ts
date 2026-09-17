export const QUEUE_TOAST_EVENT = "que:queue-toast";

export function announceQueueToast(message: string) {
  if (typeof window === "undefined" || !message) return;
  window.dispatchEvent(new CustomEvent(QUEUE_TOAST_EVENT, { detail: message }));
}
