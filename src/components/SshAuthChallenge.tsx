"use client";

import { useCallback, useState } from "react";
import { createPortal } from "react-dom";
import { useI18n } from "@/hooks/useI18n";
import { machineErrorText } from "@/lib/workspace-machine-errors";
import { WorkspaceMachineIcon } from "./WorkspaceMachineIcon";

type Challenge = { kind: "password" } | { kind: "trust"; prompt: string };

export function useSshAuthChallenge() {
  const [challenge, setChallenge] = useState<Challenge | null>(null);
  const present = useCallback((error: unknown) => {
    const failure = error as { code?: string; prompt?: string };
    if (failure.code === "HOST_TRUST_REQUIRED") {
      setChallenge({ kind: "trust", prompt: failure.prompt || "" });
      return true;
    }
    if (failure.code === "AUTH_REQUIRED") {
      setChallenge({ kind: "password" });
      return true;
    }
    return false;
  }, []);
  const clear = useCallback(() => setChallenge(null), []);
  return { challenge, present, clear };
}

export function SshAuthChallenge({ challenge, hostName, busy, error, onCancel, onRetry }: {
  challenge: Challenge | null;
  hostName: string;
  busy: boolean;
  error?: unknown;
  onCancel: () => void;
  onRetry: (password?: string, trustedPrompt?: string) => void;
}) {
  const { t } = useI18n();
  const [password, setPassword] = useState("");
  if (!challenge || typeof document === "undefined") return null;
  const trust = challenge.kind === "trust";
  // Portals retain their React ancestry: isolate events from the underlying
  // picker/form so Enter submits authentication, never another workspace.
  return createPortal(<div className="machine-auth-backdrop machine-auth-theme" onClick={event => event.stopPropagation()} onKeyUp={event => event.stopPropagation()}><form className="machine-auth-dialog" role="dialog" aria-modal="true" aria-label={t(trust ? "machines.trustTitle" : "machines.authTitle")} onKeyDown={(event) => {
    event.stopPropagation();
    if (event.key === "Escape" && !busy) { event.preventDefault(); onCancel(); return; }
    if (event.key !== "Tab") return;
    const focusable = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]')].filter((element) => element.getClientRects().length);
    const first = focusable[0], last = focusable.at(-1);
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
  }} onSubmit={(event) => {
    event.preventDefault();
    event.stopPropagation();
    if (busy) return;
    if (trust) onRetry(undefined, challenge.prompt);
    else onRetry(password);
    setPassword("");
  }}>
    <div className="machine-auth-heading"><WorkspaceMachineIcon name="lock" /><h3>{t(trust ? "machines.trustTitle" : "machines.authTitle")}</h3></div>
    <p className="machine-help">{hostName}</p>
    {trust ? <><p className="machine-help">{t("machines.trustHint")}</p><pre>{challenge.prompt}</pre></> : <><label>{t("machines.password")}<input type="password" autoFocus autoComplete="off" required value={password} onChange={(event) => setPassword(event.target.value)} /></label><p className="machine-help">{t("machines.passwordHint")}</p>{error && <p className="machine-error" role="alert">{machineErrorText(error, t)}</p>}</>}
    <div className="machine-actions"><button autoFocus={trust} className="machine-button" type="button" disabled={busy} onClick={() => { setPassword(""); onCancel(); }}>{t("machines.cancel")}</button><button className="machine-button is-primary" type="submit" disabled={busy}>{t(busy ? "machines.connecting" : trust ? "machines.trust" : "machines.connect", { name: hostName })}</button></div>
  </form></div>, document.querySelector<HTMLElement>(".cq-shell") ?? document.body);
}

export function SshConnectionWait({ hostName, busy, error, onCancel, onRetry }: {
  hostName: string;
  busy: boolean;
  error?: unknown;
  onCancel: () => void;
  onRetry: () => void;
}) {
  const { t } = useI18n();
  if (typeof document === "undefined") return null;
  return createPortal(<div className="machine-auth-backdrop machine-auth-theme" onClick={event => event.stopPropagation()} onKeyUp={event => event.stopPropagation()} onKeyDown={event => {
    event.stopPropagation();
    if (event.key === "Escape") { event.preventDefault(); onCancel(); }
  }}><section className="machine-auth-dialog machine-connection-dialog" role="dialog" aria-modal="true" aria-label={t("machines.connecting", { name: hostName })}>
    <div className="machine-connection-wait" role={error ? "alert" : "status"}>
      <span className="machine-symbol is-remote"><WorkspaceMachineIcon name="remote" size={24} /></span>
      <strong>{hostName}</strong>
      <p>{busy ? t("machines.connecting", { name: hostName }) : machineErrorText(error, t)}</p>
      <div className="machine-actions"><button className="machine-button" type="button" onClick={onCancel}>{t("machines.cancel")}</button>{!busy && <button className="machine-button is-primary" type="button" onClick={onRetry}>{t("machines.retry")}</button>}</div>
    </div>
  </section></div>, document.querySelector<HTMLElement>(".cq-shell") ?? document.body);
}
