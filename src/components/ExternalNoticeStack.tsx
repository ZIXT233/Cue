"use client";

import { useLayoutEffect, useRef, useState } from "react";
import { useI18n } from "@/hooks/useI18n";
import { copyText } from "@/lib/clipboard";
import { externalNoticeTitle, type ExternalNotice, type ExternalTurn } from "@/lib/card-queue";
import { harnessName, providerIconId } from "@/lib/harness/catalog";
import { Icon } from "./QueueIcon";
import { ScoreChipTooltip } from "./ScoreChipTooltip";
import { WorkspaceMachineIcon } from "./WorkspaceMachineIcon";
import { MarkdownMessage } from "./MarkdownMessage";

function ExternalHarnessWatermark({ kind }: { kind: string }) {
  const iconId = providerIconId(kind);
  if (kind === "shell") {
    return (
      <div className="cq-external-watermark" data-harness={kind} aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="3" />
          <path d="m7 9 3 3-3 3m6 0h4" />
        </svg>
      </div>
    );
  }
  if (kind === "pi" || kind === "omp") {
    return (
      <div className="cq-external-watermark" data-harness={kind} aria-hidden="true">
        <span className="cq-external-watermark-pi">π</span>
      </div>
    );
  }
  return (
    <div className="cq-external-watermark" data-harness={kind} aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="currentColor">
        <use href={`/provider-icons.svg#${iconId}`} />
      </svg>
    </div>
  );
}

function ExternalHarnessHeaderBadge({ kind, title }: { kind: string; title: string }) {
  const iconId = providerIconId(kind);
  if (kind === "shell") {
    return (
      <div className="cq-external-header-badge" data-harness={kind} title={title} aria-label={title}>
        <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="3" />
          <path d="m7 9 3 3-3 3m6 0h4" />
        </svg>
      </div>
    );
  }
  if (kind === "pi" || kind === "omp") {
    return (
      <div className="cq-external-header-badge" data-harness={kind} title={title} aria-label={title}>
        <span className="cq-external-header-badge-pi">π</span>
      </div>
    );
  }
  return (
    <div className="cq-external-header-badge" data-harness={kind} title={title} aria-label={title}>
      <svg viewBox="0 0 24 24" width="30" height="30" fill="currentColor">
        <use href={`/provider-icons.svg#${iconId}`} />
      </svg>
    </div>
  );
}

/**
 * A card for an attention call from a session Cue never launched. Cursor's user-level
 * hooks are global, so IDE chats and other terminals report here too. It rides the
 * deck like any other card, so it reuses the queue card's shell, header, chips and
 * actions; the body is the only thing it owns, because there is no terminal behind it
 * and the session's own record is all there is to read.
 */
export function ExternalSessionCard({ notice, isFront, host, folder, directory, onDismiss }: {
  notice: ExternalNotice;
  isFront: boolean;
  /** Host and CLI, already labelled by the shell exactly as a real card's chip is. */
  host: string;
  /** Last path segment of the directory the session runs in. */
  folder: string;
  directory: string;
  onDismiss: () => void;
}) {
  const { t } = useI18n();
  const harness = harnessName(notice.kind);
  const isWorking = notice.state === "working";
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  // A permission gate is the ask; a finished turn is already answered.
  const blocked = Boolean(notice.tool || notice.notification);
  const title = externalNoticeTitle(notice) ?? t("external.title");
  // The session's own record when it can be read; the hook's single exchange otherwise.
  // Messages carry no labels: which side said what is the alignment's job.
  const transcript: ExternalTurn[] = notice.turns?.length
    ? [...notice.turns]
    : [
        { role: "user" as const, text: notice.prompt ?? "" },
        { role: "assistant" as const, text: notice.preview ?? "" },
      ].filter((turn) => turn.text);

  // If the transcript ends on a user turn but we have an assistant preview from the hook, append it.
  if (transcript.length > 0 && transcript[transcript.length - 1].role === "user" && notice.preview) {
    transcript.push({ role: "assistant", text: notice.preview });
  }

  useLayoutEffect(() => {
    if (bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [notice.id, transcript.length]);

  return (
    <article aria-hidden={!isFront} inert={!isFront} className="cq-large-card cq-continuous-card cq-external-card"
      data-phase={isWorking ? "working" : "attention"} data-external-harness={notice.kind} data-card-id={`external:${notice.id}`}>
      <div className="cq-card-header">
        <div className="cq-card-identity">
          <div className="cq-card-meta">
            <div className="cq-card-state" title={t("external.hint", { name: harness })}>
              <i className={isWorking ? "cq-dot" : "cq-ready-dot"} />{harness}
              <span>· {t(isWorking ? "queue.正在工作" : (blocked ? "external.permission" : "external.finished"))}</span>
            </div>
          </div>
          <div className="cq-card-title-row">
            <div className="cq-title-primary">
              <h2>{title}</h2>
              <span className="cq-title-environment cq-title-host" aria-label={host}>
                <WorkspaceMachineIcon name="local" size={15} /><b>{host}</b>
              </span>
              {folder && <ScoreChipTooltip text={<div className="cq-environment-tooltip"><span><WorkspaceMachineIcon name="folder" size={14} />{folder}</span><small>{directory || folder}</small></div>}>
                <span className="cq-title-environment cq-title-workspace" aria-label={folder}>
                  <WorkspaceMachineIcon name="folder" size={15} /><b>{folder}</b>
                </span>
              </ScoreChipTooltip>}
            </div>
          </div>
        </div>
        <ExternalHarnessHeaderBadge kind={notice.kind} title={harness} />
        <div className="cq-card-actions">
          <button type="button" className="cq-action-archive" aria-label={t(isWorking ? "queue.关闭" : "external.dismiss")} onClick={onDismiss}>
            <Icon name="close" /><span className="cq-action-tooltip" role="tooltip">{t(isWorking ? "queue.关闭" : "external.dismiss")}</span>
          </button>
        </div>
      </div>
      <div className="cq-external-content-wrap">
        <ExternalHarnessWatermark kind={notice.kind} />
        <div ref={bodyRef} className="cq-harness-body cq-external-body">
          {notice.tool && <p className="cq-harness-note">{t("external.tool", { tool: notice.tool })}</p>}
          {transcript.map((turn, index) => (
            <div className="cq-external-message-wrap" data-role={turn.role} key={`${index}:${turn.role}`}>
              <div className="cq-external-message markdown-body" data-role={turn.role}>
                <MarkdownMessage content={turn.text} role={turn.role} />
              </div>
              <button
                type="button"
                className="cq-external-copy-btn"
                title={copiedIndex === index ? (t("i18n.copied") || "已复制") : (t("i18n.copy") || "复制")}
                aria-label={copiedIndex === index ? "已复制" : "复制"}
                onClick={async (e) => {
                  e.stopPropagation();
                  try {
                    await copyText(turn.text);
                    setCopiedIndex(index);
                    setTimeout(() => setCopiedIndex((prev) => (prev === index ? null : prev)), 1800);
                  } catch { /* ignore */ }
                }}
              >
                <Icon name={copiedIndex === index ? "check" : "copy"} size={12} />
                {copiedIndex === index && <span>{t("i18n.copied") || "已复制"}</span>}
              </button>
            </div>
          ))}
          {isWorking && (
            <div className="cq-external-working-indicator">
              <span className="cq-bars"><i /><i /><i /><i /></span>
              <span>{t("queue.正在工作")}</span>
            </div>
          )}
        </div>
      </div>
      <footer className="cq-external-notice-bar">
        <div className="cq-external-notice-badge">
          <span className="cq-ready-dot" />
          <span>{t("external.footerBadge")}</span>
        </div>
        <div className="cq-external-notice-body">
          <p className="cq-external-notice-primary">
            {t("external.footerManual", { name: harness })}
          </p>
          <p className="cq-external-notice-secondary">
            {t("external.footerAutoDismiss", { name: harness })}
          </p>
        </div>
      </footer>
    </article>
  );
}
