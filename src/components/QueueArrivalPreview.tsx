"use client";

import type { QueueArrivalSide } from "@/lib/queue-arrival";
import { useI18n } from "@/hooks/useI18n";

export interface QueueArrivalNotice {
  key: string;
  cardId: string;
  side: QueueArrivalSide;
  quiet?: boolean;
  title: string;
  reply: string;
}

export function QueueArrivalPreview({ notice, onOpen }: {
  notice: QueueArrivalNotice;
  onOpen: (cardId: string) => void;
}) {
  const { t } = useI18n();
  const label = notice.side === "left" ? t("queue.可能需要先查看的卡片") : "";
  return (
    <button
      key={notice.key}
      type="button"
      className="cq-queue-arrival"
      data-side={notice.side}
      aria-label={[label, notice.title, notice.reply].filter(Boolean).join(": ")}
      onClick={() => onOpen(notice.cardId)}
    >
      <span className="cq-queue-arrival-direction" aria-hidden="true">{notice.side === "left" ? "←" : "→"}</span>
      <span className="cq-queue-arrival-copy">
        {label && <small className="cq-queue-arrival-label">{label}</small>}
        <strong>{notice.title}</strong>
        <span>{notice.reply}</span>
      </span>
    </button>
  );
}
