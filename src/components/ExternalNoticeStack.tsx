"use client";

import { useI18n } from "@/hooks/useI18n";
import { harnessCatalog } from "@/lib/harness/catalog";
import type { ExternalNotice } from "@/lib/card-queue";

/**
 * A card for an attention call from a session Cue never launched. Cursor's user-level
 * hooks are global, so IDE chats and other terminals report here too. It reuses the
 * queue card's shell so it reads and slides like any other card, but it owns no
 * session: there is nothing to type into, archive or rank. It appears while that
 * session wants a human and leaves the moment it goes back to work.
 */
export function ExternalSessionCard({ notice, isFront, onDismiss }: {
  notice: ExternalNotice;
  isFront: boolean;
  onDismiss: () => void;
}) {
  const { t } = useI18n();
  const blocked = Boolean(notice.tool || notice.notification);
  const kindLabel = harnessCatalog.find((item) => item.id === notice.kind)?.name ?? notice.kind;
  return (
    <article aria-hidden={!isFront} inert={!isFront} className="cq-large-card cq-continuous-card cq-external-card"
      data-phase="attention" data-external-session={notice.kind} data-card-id={`external:${notice.id}`}>
      <div className="cq-card-header">
        <div className="cq-card-identity">
          <div className="cq-card-state"><i className="cq-ready-dot" />{t("external.badge")}</div>
        </div>
        <div className="cq-card-title-row">
          <div className="cq-title-primary">
            <h2>{notice.project ?? t("external.title")}</h2>
            <span className="cq-title-environment" aria-label={notice.kind}>
              <b>{kindLabel}</b>
            </span>
            {notice.sessionId && <span className="cq-external-id">{notice.sessionId.slice(0, 8)}</span>}
          </div>
          <div className="cq-title-controls">
            <div className="cq-title-actions"><div className="cq-title-tools">
              <button type="button" className="cq-external-dismiss" onClick={onDismiss}
                aria-label={t("external.dismiss")} title={t("external.dismiss")}>
                <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
                  <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" fill="none" />
                </svg>
              </button>
            </div></div>
          </div>
        </div>
      </div>
      <div className="cq-external-body">
        <p className="cq-external-reason">{t(blocked ? "external.permission" : "external.finished")}</p>
        {blocked && <p className="cq-external-tool">{notice.tool ?? notice.notification}</p>}
        {notice.preview && <p className="cq-external-preview">{notice.preview}</p>}
        <p className="cq-external-hint">{t("external.hint")}</p>
      </div>
    </article>
  );
}
