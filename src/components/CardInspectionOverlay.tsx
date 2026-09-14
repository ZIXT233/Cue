"use client";
import { useLayoutEffect, useState, type ReactNode, type RefObject } from "react";
import { useI18n } from "@/hooks/useI18n";

/** The backdrop belongs beside the whole layout, so it samples sidebar and deck. */
export function CardInspectionOverlay({ anchor, children, onClose }: {
  anchor: RefObject<HTMLDivElement | null>;
  children: ReactNode;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [bounds, setBounds] = useState<{ left: number; top: number; width: number; height: number } | null>(null);
  useLayoutEffect(() => {
    const stage = anchor.current;
    if (!stage) return;
    const measure = () => {
      const rect = stage.getBoundingClientRect();
      const top = window.matchMedia("(max-width: 700px)").matches ? 15 : 24;
      setBounds({ left: rect.left, top: rect.top + top, width: rect.width, height: Math.max(0, rect.height - top - 60) });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    window.addEventListener("resize", measure);
    return () => { observer.disconnect(); window.removeEventListener("resize", measure); };
  }, [anchor]);
  return (
    <div className="cq-inspection-overlay">
      <button type="button" className="cq-inspection-backdrop" aria-label={t("queue.关闭")} onClick={onClose} />
      {bounds && <div className="cq-static-card cq-inspection-card" style={bounds}>{children}</div>}
    </div>
  );
}
