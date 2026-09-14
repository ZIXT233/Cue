"use client";

import { useEffect, useRef } from "react";
import { useI18n } from "@/hooks/useI18n";
import type { UrgentCall } from "@/lib/urgent-call";

export function UrgentCallDialog({ title, details, onRead, onDismiss }: {
  title: string;
  details: UrgentCall;
  onRead: () => void;
  onDismiss: () => void;
}) {
  const { t } = useI18n();
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    dialog.showModal();
    return () => dialog.close();
  }, []);
  return <dialog ref={ref} className="cq-urgent-dialog" role="alertdialog" aria-labelledby="cq-urgent-title" onCancel={(event) => { event.preventDefault(); event.stopPropagation(); onDismiss(); }} onKeyDown={(event) => event.stopPropagation()}>
    <div className="cq-urgent-heading"><strong id="cq-urgent-title">🚨 Urgent Call</strong><button type="button" aria-label={t("queue.关闭")} onClick={onDismiss}>×</button></div>
    <h2>{title}</h2>
    <p>{t("queue.urgentAttention")}</p>
    <dl>{(["incident", "evidence", "urgency", "action"] as const).map((key) => <div key={key}><dt>{t(`queue.urgent.${key}`)}</dt><dd>{details[key]}</dd></div>)}</dl>
    <button type="button" className="cq-urgent-read" onClick={onRead}>{t("queue.urgentRead")}</button>
  </dialog>;
}
