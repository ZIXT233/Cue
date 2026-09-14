"use client";

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

function DesktopWindowControls() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const win = getCurrentWindow();
      const sync = async () => {
        if (disposed) return;
        try {
          setMaximized(await win.isMaximized());
        } catch {
          /* window may be closing */
        }
      };
      await sync();
      const un = await win.onResized(sync);
      if (disposed) un();
      else unlisten = un;
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const win = getCurrentWindow();
  return (
    <div className="desktop-window-controls" role="toolbar" aria-label="Window controls">
      <button
        type="button"
        className="dwc-min"
        aria-label="Minimize"
        onClick={() => void win.minimize()}
      />
      <button
        type="button"
        className="dwc-max"
        data-maximized={maximized ? "true" : "false"}
        aria-label={maximized ? "Restore" : "Maximize"}
        onClick={() => void win.toggleMaximize()}
      />
      <button
        type="button"
        className="dwc-close"
        aria-label="Close"
        onClick={() => void win.close()}
      />
    </div>
  );
}

export function DesktopChrome() {
  const [desktop, setDesktop] = useState(false);
  useEffect(() => {
    const bridge = (window as Window & { cueDesktop?: { platform?: string; setWindowTheme?: (dark: boolean) => void } }).cueDesktop;
    if (!bridge) return;
    setDesktop(true);
    document.documentElement.classList.add("cue-desktop");
    document.documentElement.dataset.desktopPlatform = bridge.platform;
    const sync = () => bridge.setWindowTheme?.(document.documentElement.classList.contains("dark"));
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);
  if (!desktop) return null;
  return (
    <>
      <div className="desktop-window-chrome" data-tauri-drag-region aria-hidden="true" />
      <div className="desktop-drag-sidebar" data-tauri-drag-region aria-hidden="true" />
      {document.documentElement.dataset.desktopPlatform !== "darwin" && <DesktopWindowControls />}
    </>
  );
}
