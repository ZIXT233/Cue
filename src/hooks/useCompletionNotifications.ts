"use client";

import { persistentStorage } from "../lib/persistent-storage.ts";

import { useCallback, useEffect, useRef, useState } from "react";
import { useI18n } from "@/hooks/useI18n";
import { cardTitle } from "@/lib/harness/card-title";
import { completedCards } from "@/lib/card-completion";
import { shouldShowBrowserNotification, showBrowserNotification } from "@/lib/browser-notifications";
import type { CardQueue, QueueCard } from "@/lib/card-queue";
import { notificationEnabledByDefault } from "@/lib/notification-preference";
import { latestAssistantReply } from "@/lib/queue-arrival";

const KEY = "topcard:completion-notifications";
const CHANGE_EVENT = "topcard:completion-notifications-changed";
const STATUS_EVENT = "topcard:completion-notifications-status";
let permissionRequest: Promise<NotificationPermission> | null = null;
let statusClearTimer: number | null = null;

function requestPermissionOnce(): Promise<NotificationPermission> {
  if (Notification.permission !== "default") return Promise.resolve(Notification.permission);
  permissionRequest ??= Notification.requestPermission().finally(() => { permissionRequest = null; });
  return permissionRequest;
}

export function useCompletionNotifications(queue: CardQueue | null) {
  const { t } = useI18n();
  const [enabled, setEnabled] = useState(true);
  const [status, setStatus] = useState("");
  const announceStatus = useCallback((message: string) => {
    if (statusClearTimer) { window.clearTimeout(statusClearTimer); statusClearTimer = null; }
    setStatus(message);
    window.dispatchEvent(new CustomEvent(STATUS_EVENT, { detail: message }));
  }, []);
  const previous = useRef<QueueCard[] | null>(null);
  useEffect(() => {
    const syncStatus = (event: Event) => setStatus((event as CustomEvent<string>).detail || "");
    window.addEventListener(STATUS_EVENT, syncStatus);
    return () => window.removeEventListener(STATUS_EVENT, syncStatus);
  }, []);

  useEffect(() => {
    const sync = () => {
      try {
        if (!("Notification" in window)) { setEnabled(false); return; }
        const stored = persistentStorage().getItem(KEY);
        if (stored === null) persistentStorage().setItem(KEY, "true");
        setEnabled(notificationEnabledByDefault(stored, Notification.permission));
      }
      catch { setEnabled(false); }
    };
    sync();
    window.addEventListener(CHANGE_EVENT, sync);
    window.addEventListener("storage", sync);
    window.addEventListener("focus", sync);
    return () => { window.removeEventListener(CHANGE_EVENT, sync); window.removeEventListener("storage", sync); window.removeEventListener("focus", sync); };
  }, []);

  useEffect(() => {
    if (!enabled || !window.isSecureContext || !("Notification" in window) || Notification.permission !== "default") return;
    const requestOnFirstInteraction = () => {
      void requestPermissionOnce().then((permission) => {
        if (permission === "denied") persistentStorage().setItem(KEY, "false");
        window.dispatchEvent(new Event(CHANGE_EVENT));
      }).catch(() => {});
    };
    window.addEventListener("pointerdown", requestOnFirstInteraction, { capture: true, once: true });
    return () => window.removeEventListener("pointerdown", requestOnFirstInteraction, { capture: true });
  }, [enabled]);

  const toggle = useCallback(async () => {
    const desktop = (window as Window & { topcardDesktop?: { requestNotifications?: () => Promise<string> } }).topcardDesktop;
    if (desktop?.requestNotifications) {
      const next = !enabled;
      persistentStorage().setItem(KEY, String(next));
      setEnabled(next);
      window.dispatchEvent(new Event(CHANGE_EVENT));
      if (!next) { announceStatus(""); return; }
      announceStatus(t("settings.notificationChecking"));
      void desktop.requestNotifications().then(async (desktopState) => {
        // Keep the renderer Notification API in sync with the native probe so
        // completion toasts are not dropped while permission still looks denied.
        if ("Notification" in window && Notification.permission === "default") {
          try { await requestPermissionOnce(); } catch { /* Native probe already ran. */ }
        }
        if (desktopState === "granted" || desktopState === "requested") {
          announceStatus(t("settings.notificationEnabled"));
          statusClearTimer = window.setTimeout(() => announceStatus(""), 3000);
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
      const permission = enabled ? Notification.permission : await requestPermissionOnce();
      const next = !enabled && permission === "granted";
      persistentStorage().setItem(KEY, String(next));
      setEnabled(next);
      window.dispatchEvent(new Event(CHANGE_EVENT));
      announceStatus(permission === "denied" ? "通知已被浏览器阻止，请在地址栏的网站设置中允许通知。" : "");
    } catch { announceStatus("无法开启通知，请检查浏览器的网站通知权限。"); }
  }, [announceStatus, enabled, t]);

  useEffect(() => {
    if (!queue) return;
    const completed = completedCards(previous.current, queue.cards);
    previous.current = queue.cards;
    if (!enabled || !("Notification" in window) || Notification.permission !== "granted" || !shouldShowBrowserNotification()) return;
    for (const card of completed) {
      const session = card.session;
      const url = card.detached ? `/?card=${encodeURIComponent(card.id)}` : `/?attention=${encodeURIComponent(card.id)}`;
      const key = `topcard:notified:${card.id}`;
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
          tag: `topcard:${card.id}`,
          onClick: () => {
            window.focus();
            // Soft-focus the card; location.assign would reload the whole queue.
            window.dispatchEvent(new CustomEvent("topcard:notification-click", { detail: { url } }));
          },
        });
        if (result) { try { persistentStorage().setItem(key, turn); } catch { /* Notification already delivered. */ } }
      };
      if (navigator.locks) void navigator.locks.request(key, deliver).catch(() => {});
      else void deliver();
    }
  }, [queue, enabled, t]);
  const openSystemSettings = useCallback(async () => {
    const desktop = (window as Window & { topcardDesktop?: { openNotificationSettings?: () => Promise<boolean> } }).topcardDesktop;
    if (!desktop?.openNotificationSettings || !await desktop.openNotificationSettings()) {
      announceStatus(t("settings.notificationSystemSettingsFailed"));
    }
  }, [announceStatus, t]);

  return { enabled, status, toggle, openSystemSettings, dismissStatus: () => announceStatus(""), canOpenSystemSettings: typeof window !== "undefined" && "topcardDesktop" in window };
}
