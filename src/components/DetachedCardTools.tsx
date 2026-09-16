"use client";

import { useCallback, useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useResizablePanel } from "@/hooks/useResizablePanel";
import { RIGHT_PANEL_FALLBACK_WIDTH, RIGHT_PANEL_MIN_WIDTH, RIGHT_PANEL_MAX_WIDTH } from "@/lib/panel-layout";
import { CardExtraTerminalPanes, TerminalTabBar, useCardExtraTerminals } from "./CardSideTerminal";
import { persistCardSideTerminalOpen, type CardSideTerminalRef, type RemoteShellTarget } from "@/lib/card-side-terminals";
import type { SettingsSection } from "@/lib/settings-navigation";
import { useI18n } from "@/hooks/useI18n";

export interface DetachedCardLayoutControls {
  leftToggle: ReactNode;
  rightToggle: ReactNode;
  openFile: (path: string) => void;
  toggleTools: () => void;
  toggleTerminal: () => void;
  terminalOpen: boolean;
  toolsOpen: boolean;
  toolsPanelTarget: HTMLElement | null;
}

export function DetachedCardTools({ children, cardId, cwd, remoteShell, saved, savedOpen }: {
  children: (controls: DetachedCardLayoutControls) => ReactNode;
  cardId: string;
  cwd: string;
  sessionId: string;
  remoteShell?: RemoteShellTarget;
  saved?: CardSideTerminalRef[];
  savedOpen?: boolean;
  onSettings: (section: SettingsSection) => void;
}) {
  const { t } = useI18n();
  const [rightPanel, setRightPanel] = useState<"terminal" | "tools" | null>(savedOpen ? "terminal" : null);
  const [toolsPanelTarget, setToolsPanelTarget] = useState<HTMLDivElement | null>(null);
  const extras = useCardExtraTerminals({ cardId, cwd, remoteShell, enabled: true, saved });
  const lastRightPanel = useRef<"terminal" | "tools">(savedOpen ? "terminal" : "terminal");
  const lastSavedOpen = useRef(!!savedOpen);
  // Echo guard: once the user toggles locally, our own persisted writes come
  // back as savedOpen snapshots that can lag behind newer local toggles and
  // resurrect a just-closed panel. After the first local interaction, local
  // state is the source of truth; only the initial late restore still applies.
  const userToggled = useRef(false);
  const rightWidth = useRef(RIGHT_PANEL_FALLBACK_WIDTH);
  const rightMax = useCallback(() => RIGHT_PANEL_MAX_WIDTH, []);
  const rightResize = useResizablePanel({
    ariaLabel: t("queue.resizeSideTerminal"),
    cssVariable: "--right-panel-width",
    defaultWidth: 380,
    getMaxWidth: rightMax,
    growthDirection: "left",
    maxWidth: RIGHT_PANEL_MAX_WIDTH,
    minWidth: RIGHT_PANEL_MIN_WIDTH,
    storageKey: "que:card-side-terminal-width",
    widthRef: rightWidth,
  });
  useEffect(() => { if (rightPanel) lastRightPanel.current = rightPanel; }, [rightPanel]);
  useEffect(() => {
    if (lastSavedOpen.current === !!savedOpen) return;
    lastSavedOpen.current = !!savedOpen;
    if (userToggled.current) return;
    setRightPanel((current) => {
      if (savedOpen) return "terminal";
      return current === "terminal" ? null : current;
    });
  }, [savedOpen]);
  useEffect(() => {
    if (rightPanel === "terminal") extras.ensureTab();
  }, [extras.ensureTab, rightPanel]);
  const setTerminalOpen = (open: boolean) => {
    userToggled.current = true;
    setRightPanel(open ? "terminal" : null);
    void persistCardSideTerminalOpen(cardId, open);
    if (open) extras.ensureTab();
  };
  const selectTerminal = () => {
    setTerminalOpen(rightPanel !== "terminal");
  };
  const toggleTools = () => {
    userToggled.current = true;
    setRightPanel((current) => {
      const next = current === "tools" ? null : "tools";
      if (current === "terminal" || next === "tools") void persistCardSideTerminalOpen(cardId, false);
      return next;
    });
  };
  const rightToggle = <button className="cq-panel-toggle" title={t("queue.toggleRightSidebar")} aria-label={t("queue.toggleRightSidebar")} aria-controls="detached-tools" aria-expanded={!!rightPanel} aria-pressed={!!rightPanel} onClick={() => {
    userToggled.current = true;
    setRightPanel((current) => {
      const next = current ? null : lastRightPanel.current;
      void persistCardSideTerminalOpen(cardId, next === "terminal");
      if (next === "terminal") extras.ensureTab();
      return next;
    });
  }}><ToolIcon name="sidebarRight" /></button>;
  return <div className="cq-detached-layout">
    {children({ leftToggle: null, rightToggle, openFile: () => {}, toggleTools, toggleTerminal: selectTerminal, terminalOpen: rightPanel === "terminal", toolsOpen: rightPanel === "tools", toolsPanelTarget })}
    {rightPanel && <div {...rightResize.separatorProps} aria-controls="detached-tools" className={`panel-resize-handle right-panel-resize-handle${rightResize.isResizing ? " is-resizing" : ""}`} />}
    <div ref={rightResize.panelRef} id="detached-tools" hidden={!rightPanel} className={`cq-task-panel cq-task-right right-panel-container ${rightPanel ? "right-panel-open" : "right-panel-closed"}${rightResize.isResizing ? " right-panel-resizing" : ""}`} style={{ "--right-panel-width": `${rightResize.width}px` } as CSSProperties} aria-label={t("queue.taskTools")}>
      <div className={`cq-task-panel-heading${rightPanel === "terminal" ? " cq-terminal-tabs-heading" : ""}`}>
        <div className="cq-detached-drag" data-tauri-drag-region aria-hidden="true" />
        {rightPanel === "terminal" ? (
          <TerminalTabBar
            tabs={extras.tabs}
            activeId={extras.activeId}
            onSelect={extras.setActiveId}
            onClose={(id) => {
              extras.closeTab(id);
              if (extras.tabs.filter((tab) => tab.id !== id).length === 0) setTerminalOpen(false);
            }}
            onAdd={() => extras.addTab()}
          />
        ) : <span>{t("queue.sessionTools")}</span>}
      </div>
      <div ref={setToolsPanelTarget} className="cq-task-tools-definitions" hidden={rightPanel !== "tools"} />
      {rightPanel === "terminal" && (
        <div className="cq-task-terminal">
          <CardExtraTerminalPanes
            tabs={extras.tabs}
            activeId={extras.activeId}
            active
            cardId={cardId}
            remote={!!remoteShell}
            onRestart={extras.restartTab}
            onUnavailable={(id) => extras.dropTab(id)}
          />
        </div>
      )}
    </div>
  </div>;
}

function ToolIcon({ name }: { name: string }) {
  const paths: Record<string, string> = {
    sidebarRight: "M4 3h16a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1ZM15 3v18",
  };
  return <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name]} /></svg>;
}
