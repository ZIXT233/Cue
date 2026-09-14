"use client";

import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useI18n } from "@/hooks/useI18n";

export function PriorityBadge({ weight, enabled, onSave }: {
  weight: number; enabled: boolean; onSave: (weight: number) => Promise<unknown>;
}) {
  const { t } = useI18n();
  const id = useId();
  const anchor = useRef<HTMLButtonElement>(null);
  const field = useRef<HTMLInputElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const cancelled = useRef(false);
  const [position, setPosition] = useState<{left: number; top: number} | null>(null);
  const [saving, setSaving] = useState(false);
  useEffect(() => () => clearTimeout(timer.current), []);
  const show = () => {
    clearTimeout(timer.current);
    if (!enabled || position) return;
    const rect = anchor.current?.getBoundingClientRect();
    if (!rect) return;
    cancelled.current = false;
    setPosition({left: Math.max(8, Math.min(rect.left, window.innerWidth - 208)), top: Math.max(8, Math.min(rect.bottom + 6, window.innerHeight - 110))});
  };
  const leave = () => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      if (document.activeElement !== field.current) setPosition(null);
    }, 180);
  };
  const commit = async (input: HTMLInputElement) => {
    if (cancelled.current || saving) return;
    if (!input.checkValidity()) { input.reportValidity(); return; }
    const value = input.valueAsNumber;
    if (value === weight) { setPosition(null); return; }
    setSaving(true);
    try { await onSave(value); } finally { setSaving(false); setPosition(null); }
  };
  return <>
    <button ref={anchor} type="button" className="cq-score-chip cq-score-weight cq-priority-badge" disabled={!enabled} aria-haspopup="dialog" aria-expanded={enabled && !!position} aria-controls={enabled && position ? id : undefined} onMouseEnter={show} onMouseLeave={leave} onFocus={show} onBlur={leave} onClick={show}>
      <span aria-hidden="true">⚖️</span> {t("queue.会话权重")} <b>{weight}</b>
    </button>
    {enabled && position && createPortal(<div id={id} className="cq-priority-popover" role="dialog" aria-label={t("queue.会话权重")} style={position} onMouseEnter={() => clearTimeout(timer.current)} onMouseLeave={leave} onKeyDown={event => {
      if (event.key === "Escape") { event.stopPropagation(); cancelled.current = true; setPosition(null); }
    }}>
      <label htmlFor={id + "-input"}>{t("queue.会话权重")}</label>
      <input id={id + "-input"} ref={field} type="number" step="any" min={-1000000} max={1000000} required defaultValue={weight} disabled={saving} onFocus={() => clearTimeout(timer.current)} onBlur={event => void commit(event.currentTarget)} onKeyDown={event => {
        if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); event.stopPropagation(); event.currentTarget.blur(); }
      }} />
      <small>{t("queue.priorityEditHint")}</small>
    </div>, document.body)}
  </>;
}
