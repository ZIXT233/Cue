"use client";

import { persistentStorage } from "../lib/persistent-storage.ts";

import { useCallback, useEffect, useRef, useState } from "react";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "@/hooks/useI18n";
import { cardTitle } from "@/lib/harness/card-title";
import { completedCards } from "@/lib/card-completion";
import { isTauriRuntime, shouldShowBrowserNotification, showBrowserNotification } from "@/lib/browser-notifications";
import type { CardQueue, QueueCard } from "@/lib/card-queue";
import { notificationEnabledByDefault } from "@/lib/notification-preference";
import { latestAssistantReply } from "@/lib/queue-arrival";

const KEY = "cue:completion-notifications";
const CHANGE_EVENT = "cue:completion-notifications-changed";
const STATUS_EVENT = "cue:completion-notifications-status";
let permissionRequest: Promise<NotificationPermission> | null = null;
let statusClearTimer: number | null = null;

/**
 * Current notification permission across both runtimes. In the Tauri WebView
 * the web Notification API never reaches "granted" (missing in WKWebView,
 * unusable in WebView2), so the native plugin state is the source of truth.
 */
async function currentPermissionState(): Promise<NotificationPermission | "unsupported"> {
  if (isTauriRuntime()) return (await isPermissionGranted()) ? "granted" : "default";
  if (typeof window === "undefined" || !("Notification" in window)) return "unsupported";
  return Notification.permission;
}

async function requestPermissionOnce(): Promise<NotificationPermission> {
  if (isTauriRuntime()) return requestPermission();
  if (typeof window === "undefined" || !("Notification" in window)) return "denied";
  if (Notification.permission !== "default") return Notification.permission;
  permissionRequest ??= Notification.requestPermission().finally(() => { permissionRequest = null; });
  return permissionRequest;
}

export function useCompletionNotifications(queue: CardQueue | null) {
  const { t } = useI18n();
  const [enabled, setEnabled] = useState(true);
  const [permission, setPermission] = useState<NotificationPermission>("default");
  const [status, setStatus] = useState("");
  const announceStatus = useCallback((message: string) => {
    if (statusClearTimer) { window.clearTimeout(statusClearTimer); statusClearTimer = null; }
    setStatus(message);
    window.dispatchEvent(new CustomEvent(STATUS_EVENT, { detail: message }));
  }, []);
  const previous = useRef<QueueCard[] | null>(null);
  // Last completion toast sent through the native channel; used by the focus
  // heuristic to route a toast click on platforms without a click callback.
  const recentNativeToast = useRef<{ url: string; at: number } | null>(null);

  // Precise routing: the Rust side emits this when the user clicks a toast
  // (macOS delegate callback carries the card metadata through user-notify).
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let dispose: (() => void) | undefined;
    void listen<{ cardId: string | null; sessionUrl: string }>("cue://notification-response", (event) => {
      // The native callback is authoritative; cancel the focus heuristic.
      recentNativeToast.current = null;
      window.focus();
      window.dispatchEvent(new CustomEvent("cue:notification-click", { detail: { url: event.payload.sessionUrl } }));
    }).then((unlisten) => { dispose = unlisten; }).catch(() => {});
    return () => dispose?.();
  }, []);

  // Heuristic routing for platforms without a click callback (Windows/Linux,
  // and unsigned dev builds): activating a toast brings the app to the
  // foreground, so a focus shortly after a delivered toast is treated as a
  // click on it. A short delay lets the native callback win on macOS.
  useEffect(() => {
    const routeFromFocus = () => {
      const recent = recentNativeToast.current;
      if (!recent) return;
      if (Date.now() - recent.at > 15000) { recentNativeToast.current = null; return; }
      window.setTimeout(() => {
        const pending = recentNativeToast.current;
        if (!pending || pending.url !== recent.url) return;
        recentNativeToast.current = null;
        window.dispatchEvent(new CustomEvent("cue:notification-click", { detail: { url: pending.url } }));
      }, 250);
    };
    window.addEventListener("focus", routeFromFocus);
    return () => window.removeEventListener("focus", routeFromFocus);
  }, []);

  // Keep the permission state in sync with whichever runtime backs it.
  useEffect(() => {
    let cancelled = false;
    const refresh = () => {
      currentPermissionState().then((next) => {
        if (!cancelled && next !== "unsupported") setPermission(next);
      }).catch(() => { /* Keep the last known state. */ });
    };
    refresh();
    window.addEventListener(CHANGE_EVENT, refresh);
    window.addEventListener("focus", refresh);
    return () => {
      cancelled = true;
      window.removeEventListener(CHANGE_EVENT, refresh);
      window.removeEventListener("focus", refresh);
    };
  }, []);

  useEffect(() => {
    const syncStatus = (event: Event) => setStatus((event as CustomEvent<string>).detail || "");
    window.addEventListener(STATUS_EVENT, syncStatus);
    return () => window.removeEventListener(STATUS_EVENT, syncStatus);
  }, []);

  useEffect(() => {
    const sync = () => {
      try {
        const stored = persistentStorage().getItem(KEY);
        if (stored === null) persistentStorage().setItem(KEY, "true");
        setEnabled(notificationEnabledByDefault(stored, permission));
      }
      catch { setEnabled(false); }
    };
    sync();
    window.addEventListener(CHANGE_EVENT, sync);
    window.addEventListener("storage", sync);
    return () => { window.removeEventListener(CHANGE_EVENT, sync); window.removeEventListener("storage", sync); };
  }, [permission]);

  useEffect(() => {
    if (!enabled || permission !== "default") return;
    const requestOnFirstInteraction = () => {
      void requestPermissionOnce().then((next) => {
        if (next === "denied") {
          try { persistentStorage().setItem(KEY, "false"); } catch { /* Storage unavailable. */ }
        }
        setPermission(next);
        window.dispatchEvent(new Event(CHANGE_EVENT));
      }).catch(() => {});
    };
    window.addEventListener("pointerdown", requestOnFirstInteraction, { capture: true, once: true });
    return () => window.removeEventListener("pointerdown", requestOnFirstInteraction, { capture: true });
  }, [enabled, permission]);

  // Diagnostic path: exercises the full delivery chain (invoke → user-notify
  // → plugin fallback) without the focus/completion gates. Used by the test
  // button in settings, after enabling notifications, and for verifying dev builds.
  const sendTestNotification = useCallback(async () => {
    try {
      const result = await showBrowserNotification({
        title: "Cue",
        body: t("settings.testNotificationSent"),
        sessionUrl: "/",
        cardId: "test",
        tag: `cue:test:${Date.now()}`,
        onClick: () => {},
      });
      announceStatus(result ? t("settings.testNotificationSent") : t("settings.testNotificationFailed"));
      return result;
    } catch {
      announceStatus(t("settings.testNotificationFailed"));
      return null;
    }
  }, [announceStatus, t]);

  const toggle = useCallback(async () => {
    const desktop = (window as Window & { cueDesktop?: { requestNotifications?: () => Promise<string> } }).cueDesktop;
    if (desktop?.requestNotifications) {
      const next = !enabled;
      persistentStorage().setItem(KEY, String(next));
      setEnabled(next);
      window.dispatchEvent(new Event(CHANGE_EVENT));
      if (!next) { announceStatus(""); return; }
      announceStatus(t("settings.notificationChecking"));
      void desktop.requestNotifications().then((desktopState) => {
        // The CHANGE_EVENT listener re-probes the native permission, so the
        // delivery gate below flips to "granted" without touching the web API.
        if (desktopState === "granted" || desktopState === "requested") {
          // Actually send one: the toast doubles as immediate confirmation
          // that the whole chain works (the old message merely claimed it).
          void sendTestNotification();
        }
        else if (desktopState === "settings-opened") announceStatus(t("settings.notificationSystemSettingsOpened"));
        else announceStatus(t("settings.notificationSystemSettingsFailed"));
      }).catch(() => announceStatus(t("settings.notificationSystemSettingsFailed")));
      return;
    }
    if (!window.isSecureContext || !("Notification" in window)) {
      announceStatus("系统通知需要支持通知的浏览器，并使用 localhost 或 HTTPS 访问。");
      return;
    }
    try {
      const current = enabled ? permission : await requestPermissionOnce();
      const next = !enabled && current === "granted";
      if (!enabled) setPermission(current);
      persistentStorage().setItem(KEY, String(next));
      setEnabled(next);
      window.dispatchEvent(new Event(CHANGE_EVENT));
      announceStatus(current === "denied" ? "通知已被浏览器阻止，请在地址栏的网站设置中允许通知。" : "");
    } catch { announceStatus("无法开启通知，请检查浏览器的网站通知权限。"); }
  }, [announceStatus, enabled, permission, sendTestNotification, t]);

  useEffect(() => {
    if (!queue) return;
    const completed = completedCards(previous.current, queue.cards);
    previous.current = queue.cards;
    // On the web this mirrors Notification.permission === "granted"; under
    // Tauri it reflects the native plugin permission instead.
    if (!enabled || permission !== "granted" || !shouldShowBrowserNotification()) return;
    for (const card of completed) {
      const session = card.session;
      const url = card.detached ? `/?card=${encodeURIComponent(card.id)}` : `/?attention=${encodeURIComponent(card.id)}`;
      const key = `cue:notified:${card.id}`;
      const turn = JSON.stringify([session?.id ?? card.id, card.turnKey ?? card.readyAt]);
      // Serialize across the main window and detached tabs when Web Locks is available.
      const deliver = async () => {
        try { if (persistentStorage().getItem(key) === turn) return; } catch { /* Best effort without storage. */ }
        let reply = card.harness?.replyPreview;
        // Cursor may deliver response text after stop; Codex Ready may precede its Stop hook.
        if (!reply && ["cursor", "codex"].includes(card.harness?.kind ?? "")) {
          for (let attempt = 0; attempt < 2 && !reply; attempt++) {
            await new Promise(resolve => setTimeout(resolve, 200));
            try {
              const response = await fetch("/api/card-queue", { signal: AbortSignal.timeout(500), cache: "no-store" });
              if (!response.ok) break;
              const fresh = (await response.json() as CardQueue).cards.find(item => item.id === card.id);
              // Never borrow text from a resumed session or a subsequent turn.
              if (!fresh || fresh.phase !== "attention" || fresh.readyAt !== card.readyAt || fresh.harness?.providerSessionId !== card.harness?.providerSessionId) break;
              reply = fresh.harness?.replyPreview;
            } catch { break; }
          }
        }
        if (session) {
          try {
            const response = await fetch(`/api/sessions/${encodeURIComponent(session.id)}?tail=8&deferThinking=1&deferMedia=1`, { signal: AbortSignal.timeout(1500) });
            if (response.ok) {
              const data = await response.json();
              const messages = data.context?.messages ?? [];
              reply = latestAssistantReply(messages) || reply;
            }
          } catch { /* A preview must not prevent the notification. */ }
        }
        const body = reply?.replace(/[\x00-\x1f\x7f]/g, " ").replace(/\s+/g, " ").trim().slice(0, 240)
          || (card.harness?.kind === "shell" ? t("harness.commandDone") : t("harness.attention"));
        const result = await showBrowserNotification({
          title: cardTitle(card.harness, queue.workspaces?.find((workspace) => workspace.id === card.workspaceId)?.name, t("harness.newSession")).slice(0, 100),
          body,
          sessionUrl: url,
          cardId: card.id,
          tag: `cue:${card.id}`,
          onClick: () => {
            window.focus();
            // Soft-focus the card; location.assign would reload the whole queue.
            window.dispatchEvent(new CustomEvent("cue:notification-click", { detail: { url } }));
          },
        });
        if (result === "tauri") recentNativeToast.current = { url, at: Date.now() };
        if (result) { try { persistentStorage().setItem(key, turn); } catch { /* Notification already delivered. */ } }
      };
      if (navigator.locks) void navigator.locks.request(key, deliver).catch(() => {});
      else void deliver();
    }
  }, [queue, enabled, permission, t]);

  const openSystemSettings = useCallback(async () => {
    const desktop = (window as Window & { cueDesktop?: { openNotificationSettings?: () => Promise<boolean> } }).cueDesktop;
    if (!desktop?.openNotificationSettings || !await desktop.openNotificationSettings()) {
      announceStatus(t("settings.notificationSystemSettingsFailed"));
    }
  }, [announceStatus, t]);

  return { enabled, status, toggle, openSystemSettings, sendTestNotification, dismissStatus: () => announceStatus(""), canOpenSystemSettings: typeof window !== "undefined" && "cueDesktop" in window };
}
