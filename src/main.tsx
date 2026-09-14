import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { CardQueueShell } from "./components/CardQueueShell";
import { DesktopChrome } from "./components/DesktopChrome";
import { I18nProvider } from "./hooks/useI18n";
import { openDetachedCardWindow } from "./lib/card-window";
import type { CueDesktop } from "./lib/desktop";
import { setDeveloperProbesEnabled } from "./lib/developer-probes";
import { installApiInterceptor, setApiBase } from "./lib/http";
import "./styles/globals.css";
import "./styles/settings.css";
import "./styles/card-queue.css";

function installDesktopBridge() {
  const desktop: CueDesktop = {
    storage: window.localStorage,
    platform: navigator.userAgent.includes("Mac") ? "darwin" : navigator.userAgent.includes("Win") ? "win32" : "linux",
    owner: crypto.randomUUID(),
    writeClipboardText: async (text) => {
      try {
        await writeText(text);
        return true;
      } catch {
        return false;
      }
    },
    openCard: (cardId) => openDetachedCardWindow(cardId),
    focus: () => window.focus(),
    openNotification: (url) => {
      window.dispatchEvent(new CustomEvent("cue:notification-click", { detail: { url } }));
    },
    requestNotifications: async () => {
      if (await isPermissionGranted()) return "granted";
      return requestPermission();
    },
    openNotificationSettings: async () => {
      // The Rust command bypasses the opener plugin's frontend URL scope,
      // which rejects system schemes like ms-settings:.
      try {
        await invoke("open_notification_settings");
        return true;
      } catch {
        return false;
      }
    },
  };
  window.cueDesktop = desktop;
  document.documentElement.classList.add("cue-desktop");
  document.documentElement.dataset.desktopPlatform = desktop.platform;
}

async function boot() {
  installDesktopBridge();
  const base = await invoke<string>("api_base");
  setApiBase(base);
  installApiInterceptor();
  window.__CUE_API_BASE__ = base;
  try {
    const response = await fetch("/api/tools/settings");
    const data = await response.json() as { developerProbes?: boolean };
    setDeveloperProbesEnabled(!!data.developerProbes);
  } catch {
    /* Keep the Vite default until settings load. */
  }

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <I18nProvider>
        <DesktopChrome />
        <CardQueueShell />
      </I18nProvider>
    </React.StrictMode>,
  );
}

void boot();
