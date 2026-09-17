import { invoke } from "@tauri-apps/api/core";
import type { BlockingExtensionUiRequest, ExtensionUiRequest } from "./types";

export type NotificationDelivery = "tauri" | "service-worker" | "window" | null;

/**
 * True when the frontend runs inside the Tauri WebView. The web Notification
 * API is unusable there (missing in WKWebView, never granted in WebView2), so
 * delivery must go through the native notification plugin instead.
 */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined"
    && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window || window.queDesktop !== undefined);
}

interface WindowNotificationLike {
  onclick: Notification["onclick"];
  close: () => void;
}

interface ServiceWorkerRegistrationLike {
  showNotification: (title: string, options?: NotificationOptions) => Promise<void>;
}

export interface BrowserNotificationEnvironment {
  createWindowNotification: (title: string, options?: NotificationOptions) => WindowNotificationLike;
  getServiceWorkerRegistration: (() => Promise<ServiceWorkerRegistrationLike | undefined>) | null;
}

export interface BrowserNotificationOptions {
  title: string;
  body: string;
  sessionUrl: string;
  cardId?: string;
  onClick: () => void;
  tag?: string;
}

type DocumentAttentionState = Pick<Document, "visibilityState" | "hasFocus">;

export function shouldShowBrowserNotification(
  attentionState: DocumentAttentionState = document,
): boolean {
  return attentionState.visibilityState !== "visible" || !attentionState.hasFocus();
}

export function isBlockingExtensionUiRequest(
  request: ExtensionUiRequest,
): request is BlockingExtensionUiRequest {
  switch (request.method) {
    case "select":
    case "confirm":
    case "input":
    case "editor":
      return true;
    case "custom":
      return request.closed !== true;
    default:
      return false;
  }
}

export function claimExtensionAttentionNotification(
  request: ExtensionUiRequest,
  notifiedRequestIds: Set<string>,
): request is BlockingExtensionUiRequest {
  if (!isBlockingExtensionUiRequest(request) || notifiedRequestIds.has(request.id)) return false;
  notifiedRequestIds.add(request.id);
  return true;
}

function getBrowserEnvironment(): BrowserNotificationEnvironment {
  return {
    createWindowNotification: (title, options) => new Notification(title, options),
    getServiceWorkerRegistration: "serviceWorker" in navigator
      ? () => navigator.serviceWorker.getRegistration()
      : null,
  };
}

export async function showBrowserNotification(
  options: BrowserNotificationOptions,
  environment: BrowserNotificationEnvironment = getBrowserEnvironment(),
): Promise<NotificationDelivery> {
  // Inside the Tauri WebView the web Notification API cannot reach the OS
  // notification center, so the native channel is the only working one. The
  // Rust command routes through user-notify (click callback on macOS) and
  // falls back to the tauri notification plugin where that is unsupported.
  if (isTauriRuntime()) {
    try {
      await invoke("send_completion_notification", {
        title: options.title,
        body: options.body,
        cardId: options.cardId ?? "",
        sessionUrl: options.sessionUrl,
      });
      return "tauri";
    } catch (err) {
      console.warn("[que] native notification command failed:", err);
      return null;
    }
  }

  const notificationOptions: NotificationOptions = {
    body: options.body,
    icon: "/icons/que-192.png",
    // Que owns the completion sound; the system notification is visual only.
    silent: true,
    ...(options.tag ? { tag: options.tag, renotify: true } : {}),
  };

  if (environment.getServiceWorkerRegistration) {
    try {
      const registration = await environment.getServiceWorkerRegistration();
      if (registration) {
        await registration.showNotification(options.title, {
          ...notificationOptions,
          data: { url: options.sessionUrl },
        });
        return "service-worker";
      }
    } catch {
      // Fall back to a page notification where the constructor is supported.
    }
  }

  try {
    const notification = environment.createWindowNotification(options.title, notificationOptions);
    notification.onclick = () => {
      notification.close();
      const desktop = typeof window !== "undefined"
        ? (window as Window & { queDesktop?: { openNotification?: (url: string) => void } }).queDesktop
        : undefined;
      if (desktop?.openNotification) {
        desktop.openNotification(options.sessionUrl);
        return;
      }
      options.onClick();
    };
    return "window";
  } catch {
    // Most mobile browsers expose Notification but require service-worker delivery.
    return null;
  }
}
