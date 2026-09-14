"use client";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createShellProbe } from "@/lib/harness/shell-probe";
import { harnessPicker } from "@/lib/harness/catalog";
import { createPortal } from "react-dom";
import { useI18n } from "@/hooks/useI18n";
import { ProviderIcon } from "./ProviderIcon";
import { TerminalPanel, type TerminalConnectionStatus } from "./TerminalPanel";
import { ErrorDialog } from "./ErrorDialog";
import { HarnessDebugPanel } from "./HarnessDebugPanel";
import { developerProbesEnabled } from "@/lib/developer-probes";
import type { QueueCard } from "@/lib/card-queue";

export function HarnessCard({ card, active, inQueue = false, children, onAction, onStartAll }: {
  card: QueueCard; active: boolean; inQueue?: boolean; children?: ReactNode;
  onAction: (action: string, data: Record<string, unknown>) => Promise<boolean>;
  onStartAll?: () => Promise<void>;
}) {
  const { t } = useI18n();
  const developerProbes = developerProbesEnabled();
  const labels = { starting:t("harness.starting"), working:t("harness.working"), attention:t("harness.waiting"), unknown:t("harness.unknown"), exited:t("harness.processNotStarted"), error:t("harness.disconnected") };
  const [target, setTarget] = useState<Element | null>(null);
  const [switchPosition, setSwitchPosition] = useState<{ left: number; top: number } | null>(null);
  const [mode] = useState<"cli">("cli");
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState("");
  const [terminalStatus, setTerminalStatus] = useState<TerminalConnectionStatus>("connecting");
  const [connection, setConnection] = useState(0);
  const [showTranscript, setShowTranscript] = useState(false);
  const [showDebug, setShowDebug] = useState(false);
  const harness = card.harness;
  const [canBackground, setCanBackground] = useState(false);
  const shellTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const shellProbe = useRef<ReturnType<typeof createShellProbe> | null>(null);
  if (!shellProbe.current) shellProbe.current = createShellProbe(running => {
    clearTimeout(shellTimer.current);
    setCanBackground(false);
    if (running) shellTimer.current = setTimeout(() => setCanBackground(true), 300);
  });
  useEffect(() => {
    if (harness?.kind !== "shell" || harness.shellCommandNotifications === false) return;
    clearTimeout(shellTimer.current);
    setCanBackground(false);
    if (harness.shellCommandRunning) shellTimer.current = setTimeout(() => setCanBackground(true), Math.max(0, 300 - (Date.now() - (harness.shellCommandStartedAt ?? Date.now()))));
    return () => clearTimeout(shellTimer.current);
  }, [harness?.kind, harness?.shellCommandNotifications, harness?.shellCommandStartedAt, harness?.shellCommandRunning]);
  const ended = !!harness && (["exited", "error"].includes(harness.state) || terminalStatus === "exited");
  const disconnected = !!harness && (ended || terminalStatus === "error" || terminalStatus === "connecting");
  useEffect(() => { setShowTranscript(false); }, [disconnected, harness?.terminalId]);
  const fresh = !card.session && !harness;
  const probeDetail = harness?.kind === "shell" ? (harness.shellCommandNotifications === false ? t("harness.plainShell") : t("harness.shellHint"))
    : harness?.probe === "title-only" ? t("harness.titleProbe")
    : harness?.probe === "unconfirmed" ? t("harness.noSignal")
    : t("harness.hookSeen");

  useLayoutEffect(() => {
    const sync = () => {
      const cardEl = document.querySelector(`[data-card-id="${card.id}"]`);
      setTarget(cardEl?.querySelector(".cq-title-harness") ?? null);
      if (!fresh || !active || !cardEl) { setSwitchPosition(null); return; }
      const rect = cardEl.getBoundingClientRect();
      if (rect.width < 8 || rect.height < 8) { setSwitchPosition(null); return; }
      setSwitchPosition({
        left: Math.max(8, Math.round(rect.left - 78)),
        top: Math.round(rect.top + Math.min(168, Math.max(96, rect.height * 0.22))),
      });
    };
    sync();
    window.addEventListener("resize", sync);
    window.addEventListener("scroll", sync, true);
    const cardEl = document.querySelector(`[data-card-id="${card.id}"]`);
    const observer = typeof ResizeObserver !== "undefined" && cardEl ? new ResizeObserver(sync) : null;
    if (cardEl && observer) observer.observe(cardEl);
    return () => {
      window.removeEventListener("resize", sync);
      window.removeEventListener("scroll", sync, true);
      observer?.disconnect();
    };
  }, [card.id, fresh, active, mode, harness?.state]);

  const act = async (action: string, data: Record<string, unknown> = {}) => {
    setBusy(true);
    setActionError("");
    try { await onAction(action, { id: card.id, ...data }); }
    catch (error) { setActionError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };

  const controls = fresh ? null : harness ? <div className="cq-harness-controls">
    {harness.kind === "shell" && harness.shellCommandNotifications !== false && canBackground && !harness.shellNotify && !["error", "exited"].includes(harness.state) && <button type="button" disabled={busy} onClick={() => void act("shell_background")} title={t("harness.backgroundHint")}>↓ {t("harness.background")}</button>}
    {!disconnected && harness.state !== "attention" && <span className="cq-harness-state" data-state={harness.state} title={probeDetail}>{labels[harness.state]}</span>}
    {developerProbes && <button type="button" className="cq-harness-debug" disabled={busy} onClick={() => setShowDebug(true)} title={t("harness.debugHint")}>{t("harness.debug")}</button>}
  </div> : null;
  const terminal = harness ? <TerminalPanel key={`${harness.terminalId}:${connection}`} embedded remote={harness.remote} themeProfile={harness.kind === "grok" ? "grok" : undefined} readOnly={card.archivedAt !== undefined || harness.state === "exited" || harness.state === "error"} tab={{ id: harness.terminalId, cwd: card.cwd, restored: true }} active={active} focusReporting={harness.kind !== "shell"} inQueue={inQueue}
    onOutput={harness.kind === "shell" && harness.shellCommandNotifications !== false ? data => shellProbe.current?.(data) : undefined} onStatusChange={setTerminalStatus} onRestart={() => void act(harness.providerSessionId ? "harness_resume" : "harness_reopen")} onClosed={() => {}} onCloseError={() => {}} /> : null;

  return <>
    {actionError && <ErrorDialog message={actionError} onDismiss={() => setActionError("")} />}
    {developerProbes && showDebug && harness && <HarnessDebugPanel terminalId={harness.terminalId} onClose={() => setShowDebug(false)} />}
    {fresh ? active && switchPosition && createPortal(<div className="cq-harness-floating" style={switchPosition}>{controls}</div>, document.body) : target && controls && createPortal(controls, target)}
    {harness ? <div className="cq-harness-body" data-harness={harness.kind} data-disconnected={disconnected && !showTranscript ? "true" : undefined}>
      {disconnected && !showTranscript && <div className="cq-terminal-recovery" role="status">
        <div className="cq-terminal-recovery-content">
          <svg aria-hidden="true" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="m7 9 3 3-3 3m6 0h4"/></svg>
          <h3>{ended ? t(harness.kind === "shell" ? "harness.shellNotStarted" : "harness.processNotStarted") : terminalStatus === "connecting" ? t("harness.connecting") : t("harness.disconnected")}</h3>
          <p>{ended ? t("harness.recordsKept") : t("harness.reconnectHint")}</p>
          {actionError && <p role="alert">{actionError}</p>}
          <div className="cq-terminal-recovery-actions">
            <button type="button" className="cq-terminal-recovery-primary" disabled={busy} onClick={() => {
              if (ended) void act(harness.providerSessionId ? "harness_resume" : "harness_reopen");
              else { setTerminalStatus("connecting"); setConnection(key => key + 1); }
            }}>{busy ? t("harness.opening") : ended ? harness.providerSessionId ? t("harness.resume") : t("harness.startProcess") : t("harness.reconnect")}</button>
            {ended && onStartAll && <button type="button" className="cq-terminal-recovery-all" disabled={busy} onClick={() => void (async () => {
              setBusy(true);
              setActionError("");
              try { await onStartAll(); }
              catch (error) { setActionError(error instanceof Error ? error.message : String(error)); }
              finally { setBusy(false); }
            })()}>{t("harness.startAllProcesses")}</button>}
          </div>
          <button type="button" onClick={() => setShowTranscript(true)}>{t("harness.viewOutput")}</button>
        </div>
      </div>}
      {disconnected && showTranscript && <button type="button" className="cq-terminal-recovery-return" onClick={() => setShowTranscript(false)}>{t("harness.backToConnection")}</button>}
      {terminal}
    </div> : mode === "cli" ? <div className="cq-harness-empty"><div className="cq-harness-picker">
      <h3>{t("harness.choose")}</h3>
      <p>{t("harness.chooseHint")}</p>
      <div className="cq-harness-options">
      {harnessPicker.map(item => <button key={item.id} type="button" className="cq-harness-option" disabled={busy} onClick={() => void act("harness_start", { kind: item.id })}>
        <span className="cq-harness-option-icon" aria-hidden="true">{item.id === "shell" ? <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="m7 9 3 3-3 3m6 0h4"/></svg> : item.id === "pi" || item.id === "omp" ? <span className="cq-harness-pi-mark">π</span> : <ProviderIcon id={({ codex: "openai", claude: "anthropic", gemini: "google", antigravity: "google" } as Record<string, string>)[item.id] ?? item.id} size={28} />}</span>
        <span><strong>{item.name}</strong><small>{busy ? t("harness.checking") : item.id === "shell" ? t("harness.shellDescription") : item.description}</small></span><span className="cq-harness-option-arrow" aria-hidden="true">↗</span>
      </button>)}
      </div>
    </div></div> : children}
  </>;
}
