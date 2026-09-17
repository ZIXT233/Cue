"use client";

import { useEffect, useRef, useState } from "react";
import { useI18n } from "@/hooks/useI18n";
import { ProviderIcon } from "./ProviderIcon";
import { providerIconId } from "@/lib/harness/catalog";

export type TerminalStartupStage = "preparing" | "connecting" | "waiting_output" | "ready" | "error";

interface Props {
  stage: TerminalStartupStage;
  remote?: boolean;
  sshHost?: string;
  harnessKind?: string;
  harnessName?: string;
  error?: string | null;
  onRetry?: () => void;
  startedAt?: number;
}

const STAGE_ORDER: Record<TerminalStartupStage, number> = {
  preparing: 1,
  connecting: 2,
  waiting_output: 3,
  ready: 3,
  error: -1,
};

export function TerminalStartupProgress({
  stage,
  remote = false,
  sshHost,
  harnessKind,
  harnessName,
  error,
  onRetry,
  startedAt,
}: Props) {
  const { t } = useI18n();
  const [elapsed, setElapsed] = useState("0.0");
  const [fading, setFading] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const initialTimeRef = useRef<number>(startedAt ?? Date.now());

  // Live timer while connecting
  useEffect(() => {
    if (stage === "ready" || stage === "error" || dismissed) return;
    const update = () => {
      const ms = Math.max(0, Date.now() - initialTimeRef.current);
      setElapsed((ms / 1000).toFixed(1));
    };
    update();
    const timer = setInterval(update, 100);
    return () => clearInterval(timer);
  }, [stage, dismissed]);

  // Smooth fade-out when ready
  useEffect(() => {
    if (stage !== "ready") {
      setFading(false);
      setDismissed(false);
      return;
    }
    const fadeTimer = setTimeout(() => setFading(true), 320);
    const dismissTimer = setTimeout(() => setDismissed(true), 680);
    return () => {
      clearTimeout(fadeTimer);
      clearTimeout(dismissTimer);
    };
  }, [stage]);

  if (dismissed) return null;

  const currentStep = stage === "error" ? 1 : STAGE_ORDER[stage] ?? 1;

  const steps = [
    {
      step: 1,
      title: t("terminal.progress.step1"),
      desc: remote && sshHost
        ? t("terminal.progress.step1RemoteDesc", { host: sshHost })
        : t("terminal.progress.step1Desc"),
    },
    {
      step: 2,
      title: t("terminal.progress.step2"),
      desc: t("terminal.progress.step2Desc"),
    },
    {
      step: 3,
      title: t("terminal.progress.step3"),
      desc: stage === "ready" ? t("terminal.progress.step4Desc") : t("terminal.progress.step3Desc"),
    },
  ];

  const activeDesc = stage === "error"
    ? (error || t("terminal.progress.failed"))
    : stage === "ready"
    ? t("terminal.progress.step4Desc")
    : steps[Math.min(2, Math.max(0, currentStep - 1))].desc;

  const displayName = harnessName || (harnessKind ? harnessKind.toUpperCase() : t("terminal.title"));

  return (
    <div
      className={`cq-terminal-startup-progress${fading ? " is-fading" : ""}${stage === "error" ? " is-error" : ""}`}
      role="status"
      aria-live="polite"
    >
      {/* Top continuous segmented progress bar */}
      <div className="cq-tsp-top-bar" aria-hidden="true">
        {steps.map((s) => {
          const isDone = stage === "ready" || currentStep > s.step;
          const isActive = stage !== "ready" && stage !== "error" && currentStep === s.step;
          const isFail = stage === "error" && currentStep === s.step;
          return (
            <div
              key={s.step}
              className={`cq-tsp-segment${isDone ? " is-done" : ""}${isActive ? " is-active" : ""}${isFail ? " is-error" : ""}`}
            >
              <div className="cq-tsp-segment-fill" />
            </div>
          );
        })}
      </div>

      {/* Centered HUD Card */}
      <div className="cq-tsp-card">
        <div className="cq-tsp-header">
          <span className="cq-tsp-icon" aria-hidden="true">
            {harnessKind && harnessKind !== "shell" ? (
              <ProviderIcon id={providerIconId(harnessKind)} size={24} />
            ) : (
              <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                <rect x="3" y="4" width="18" height="16" rx="3" />
                <path d="m7 9 3 3-3 3m6 0h4" />
              </svg>
            )}
          </span>
          <div className="cq-tsp-title-group">
            <span className="cq-tsp-title">{displayName}</span>
            <span className="cq-tsp-subtitle">
              {stage === "error" ? t("terminal.progress.failed") : t("terminal.progress.title")}
            </span>
          </div>
          {remote && sshHost && (
            <span className="cq-tsp-remote-badge" title={sshHost}>
              SSH: {sshHost}
            </span>
          )}
          <span className="cq-tsp-timer">{t("terminal.progress.elapsed", { time: elapsed })}</span>
        </div>

        {/* Step checkpoints row */}
        <div className="cq-tsp-steps">
          {steps.map((s) => {
            const isDone = stage === "ready" || currentStep > s.step;
            const isActive = stage !== "ready" && stage !== "error" && currentStep === s.step;
            const isFail = stage === "error" && currentStep === s.step;

            return (
              <div
                key={s.step}
                className={`cq-tsp-step${isDone ? " is-done" : ""}${isActive ? " is-active" : ""}${isFail ? " is-error" : ""}`}
              >
                <div className="cq-tsp-step-dot">
                  {isDone ? (
                    <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  ) : isFail ? (
                    <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
                      <line x1="18" y1="6" x2="6" y2="18" />
                      <line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                  ) : (
                    <span>{s.step}</span>
                  )}
                </div>
                <span className="cq-tsp-step-label">{s.title}</span>
              </div>
            );
          })}
        </div>

        {/* Active Stage Description */}
        <div className="cq-tsp-footer">
          <p className="cq-tsp-desc">{activeDesc}</p>
          {stage === "error" && onRetry && (
            <button type="button" className="cq-tsp-retry" onClick={onRetry}>
              {t("terminal.reconnect")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
