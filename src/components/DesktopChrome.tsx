"use client";

import { useEffect, useState } from "react";

export function DesktopChrome() {
  const [desktop, setDesktop] = useState(false);
  useEffect(() => {
    const bridge = (window as Window & { topcardDesktop?: { platform?: string; setWindowTheme?: (dark: boolean) => void } }).topcardDesktop;
    if (!bridge) return;
    setDesktop(true);
    document.documentElement.classList.add("topcard-desktop");
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
    </>
  );
}
