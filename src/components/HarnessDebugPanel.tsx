import { useEffect, useState } from "react";
import { useI18n } from "@/hooks/useI18n";
import { copyText } from "@/lib/clipboard";
import { getXtermProbe, type XtermProbe } from "@/lib/terminal-probe";

interface DebugEvent {
  at: number;
  source: string;
  event: string;
  state: string;
  sessionId?: string;
  prompt?: string;
  note?: string;
}

interface DebugSnapshot {
  terminalId: string;
  signalDir: string;
  probe?: {
    state: string;
    source?: string;
    hookSeen: boolean;
    titleSeen: boolean;
    sessionId?: string;
    sessionName?: string;
    submitPrompt?: string;
    notifyOscSeen?: string[];
    notifyOscHits?: number;
  };
  events: DebugEvent[];
  trace: Record<string, unknown>[];
  lastStopDiagnostic?: Record<string, unknown>;
  pty?: Record<string, unknown>;
  clues?: string[];
  xterm?: XtermProbe;
}

function formatTime(at: number) {
  const date = new Date(at);
  return `${date.toLocaleTimeString()} .${String(date.getMilliseconds()).padStart(3, "0")}`;
}

export function HarnessDebugPanel({ terminalId, onClose }: { terminalId: string; onClose: () => void }) {
  const { t } = useI18n();
  const [data, setData] = useState<DebugSnapshot | null>(null);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const response = await fetch(`/api/harness/${encodeURIComponent(terminalId)}/debug`);
        const body = await response.json() as DebugSnapshot & { error?: string };
        if (!response.ok) throw new Error(body.error || `HTTP ${response.status}`);
        if (!cancelled) { setData({ ...body, xterm: getXtermProbe(terminalId) }); setError(""); }
      } catch (cause) {
        if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause));
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), 800);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [terminalId]);

  const copy = async () => {
    if (!data) return;
    try {
      await copyText(JSON.stringify(data, null, 2));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch { /* Keep the panel open if clipboard is blocked. */ }
  };

  return (
    <div className="cq-overlay" role="presentation" onClick={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section className="cq-dialog cq-debug-dialog" role="dialog" aria-modal="true" aria-labelledby="cq-debug-title" onClick={(event) => event.stopPropagation()}>
        <div className="cq-dialog-heading">
          <strong id="cq-debug-title">{t("harness.debugTitle")}</strong>
          <button type="button" aria-label={t("i18n.close")} onClick={onClose}>×</button>
        </div>
        <p>{t("harness.debugHint")}</p>
        <div className="cq-debug-scroll">
        {error && <pre className="cq-error-body">{error}</pre>}
        {data && <>
          {data.clues && data.clues.length > 0 && <>
            <h3>clues</h3>
            <ul className="cq-debug-empty">
              {data.clues.map((clue) => <li key={clue}>{clue}</li>)}
            </ul>
          </>}
          {data.pty && <>
            <h3>pty</h3>
            <pre className="cq-error-body">{JSON.stringify(data.pty, null, 2)}</pre>
          </>}
          {data.xterm && <>
            <h3>xterm</h3>
            <pre className="cq-error-body">{JSON.stringify(data.xterm, null, 2)}</pre>
          </>}
          <dl className="cq-debug-meta">
            <dt>{t("harness.debugTerminal")}</dt><dd>{data.terminalId}</dd>
            <dt>{t("harness.debugSignalDir")}</dt><dd>{data.signalDir}</dd>
            <dt>{t("harness.debugProbe")}</dt>
            <dd>
              {data.probe
                ? `${data.probe.state} · ${data.probe.source || "—"} · hooks=${data.probe.hookSeen ? "yes" : "no"} · titleProbe=${data.probe.titleSeen ? "yes" : "no"} · session=${data.probe.sessionName || "—"} · prompt=${data.probe.submitPrompt || "—"} · osc=${data.probe.notifyOscSeen?.join("/") || "none"}${data.probe.notifyOscHits ? `×${data.probe.notifyOscHits}` : ""}`
                : t("harness.debugNoProbe")}
            </dd>
            {data.probe?.sessionId && <><dt>session</dt><dd>{data.probe.sessionId}</dd></>}
          </dl>
          <h3>{t("harness.debugEvents")}</h3>
          {data.events.length === 0 ? <p className="cq-debug-empty">{t("harness.debugEmpty")}</p> : (
            <div className="cq-debug-table-wrap">
              <table className="cq-debug-table">
                <thead>
                  <tr>
                    <th>time</th>
                    <th>source</th>
                    <th>event</th>
                    <th>state</th>
                    <th>note</th>
                    <th>prompt</th>
                  </tr>
                </thead>
                <tbody>
                  {data.events.map((event, index) => (
                    <tr key={`${event.at}-${index}`}>
                      <td>{formatTime(event.at)}</td>
                      <td>{event.source}</td>
                      <td>{event.event}</td>
                      <td>{event.state}</td>
                      <td className="cq-debug-wide">{event.note || ""}</td>
                      <td className="cq-debug-wide">{event.prompt || ""}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {data.trace.length > 0 && <>
            <h3>{t("harness.debugTrace")}</h3>
            <pre className="cq-error-body">{JSON.stringify(data.trace, null, 2)}</pre>
          </>}
          {data.lastStopDiagnostic && <>
            <h3>{t("harness.debugStop")}</h3>
            <pre className="cq-error-body">{JSON.stringify(data.lastStopDiagnostic, null, 2)}</pre>
          </>}
        </>}
        </div>
        <div className="cq-error-actions">
          <button type="button" className="cq-error-copy" onClick={() => void copy()}>{copied ? t("error.copied") : t("error.copy")}</button>
          <button type="button" onClick={onClose}>{t("error.dismiss")}</button>
        </div>
      </section>
    </div>
  );
}
