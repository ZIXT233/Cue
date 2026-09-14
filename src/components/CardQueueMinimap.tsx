"use client";

import { useEffect, useRef, useState, type PointerEvent } from "react";
import { WorkspaceMachineIcon } from "./WorkspaceMachineIcon";

type CardQueueMinimapProps = {
  cards: Array<{ id: string; sessionId?: string; title: string; excerpt?: string; host: string; workspace: string; remote: boolean }>;
  activeIndex: number;
  label: string;
  itemLabel: (index: number, title: string) => string;
  onSelect: (index: number) => void;
};

export function CardQueueMinimap({ cards, activeIndex, label, itemLabel, onSelect }: CardQueueMinimapProps) {
  const navRef = useRef<HTMLElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ id: number; index: number } | null>(null);
  const [preview, setPreview] = useState<{ index: number; left: number } | null>(null);
  const [summaries, setSummaries] = useState<Record<string, string>>({});
  const previewCard = preview ? cards[preview.index] : undefined;

  const showPreview = (index: number, target: HTMLButtonElement) => {
    const nav = navRef.current;
    if (!nav) return;
    const navRect = nav.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const center = targetRect.left - navRect.left + targetRect.width / 2;
    const halfPreview = Math.min(210, navRect.width / 2);
    setPreview({ index, left: Math.max(halfPreview, Math.min(navRect.width - halfPreview, center)) });
  };

  useEffect(() => {
    const track = trackRef.current;
    const current = track?.querySelector<HTMLElement>("[aria-current='true']");
    if (!track || !current || dragRef.current) return;
    const left = current.offsetLeft - (track.clientWidth - current.offsetWidth) / 2;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    track.scrollTo({ left: Math.max(0, left), behavior: reduceMotion ? "auto" : "smooth" });
  }, [activeIndex]);

  useEffect(() => {
    const sessionId = previewCard?.sessionId;
    if (!sessionId || summaries[sessionId] !== undefined) return;
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      void fetch(`/api/sessions/${encodeURIComponent(sessionId)}?tail=8&deferThinking=1&deferMedia=1`, {
        cache: "no-store",
        signal: controller.signal,
      }).then((response) => response.ok ? response.json() : Promise.reject(new Error("preview unavailable")))
        .then((data: { context?: { messages?: Array<{ role?: string; content?: unknown }> } }) => {
          const messages = data.context?.messages ?? [];
          const assistant = [...messages].reverse().find((message) => message.role === "assistant");
          const content = Array.isArray(assistant?.content)
            ? assistant.content.filter((block): block is { type: "text"; text: string } => {
                if (!block || typeof block !== "object") return false;
                const item = block as { type?: unknown; text?: unknown };
                return item.type === "text" && typeof item.text === "string";
              }).map((block) => block.text).join(" ")
            : "";
          const plainText = content
            .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
            .replace(/[`*_>#]/g, "")
            .replace(/\s+/g, " ")
            .trim();
          setSummaries((current) => ({ ...current, [sessionId]: plainText }));
        })
        .catch((error: unknown) => {
          if ((error as { name?: string }).name !== "AbortError") setSummaries((current) => ({ ...current, [sessionId]: "" }));
        });
    }, 120);
    return () => { window.clearTimeout(timer); controller.abort(); };
  }, [previewCard?.sessionId, summaries]);

  const selectAtPointer = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.id !== event.pointerId) return;
    const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button"));
    let closest = -1;
    let distance = Infinity;
    buttons.forEach((button, index) => {
      const rect = button.getBoundingClientRect();
      const next = Math.abs(event.clientX - (rect.left + rect.width / 2));
      if (next < distance) { distance = next; closest = index; }
    });
    if (closest < 0) return;
    showPreview(closest, buttons[closest]);
    if (closest !== drag.index) {
      drag.index = closest;
      onSelect(closest);
    }
  };
  const endDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.id !== event.pointerId) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    setPreview(null);
  };

  if (!cards.length) return null;

  return (
    <nav className="cq-queue-minimap" aria-label={label} ref={navRef}>
      <div className="cq-queue-minimap-track" ref={trackRef}
        style={{ touchAction: "none" }}
        onPointerDown={(event) => {
          if (!event.isPrimary || event.button !== 0) return;
          dragRef.current = { id: event.pointerId, index: -1 };
          event.currentTarget.setPointerCapture(event.pointerId);
          event.preventDefault();
          selectAtPointer(event);
        }}
        onPointerMove={selectAtPointer}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onLostPointerCapture={endDrag}
      >
        {cards.map((card, index) => {
          const current = index === activeIndex;
          const proximity = preview ? Math.abs(index - preview.index) : Number.POSITIVE_INFINITY;
          const accessibleLabel = itemLabel(index, card.title);
          return (
            <button
              key={card.id}
              type="button"
              className={`${current ? "is-current " : ""}${proximity <= 3 ? `is-near-${proximity}` : ""}`.trim() || undefined}
              aria-current={current ? "true" : undefined}
              aria-label={accessibleLabel}
              onMouseEnter={(event) => showPreview(index, event.currentTarget)}
              onMouseLeave={() => setPreview(null)}
              onFocus={(event) => showPreview(index, event.currentTarget)}
              onBlur={() => setPreview(null)}
              onClick={(event) => { if (event.detail === 0) onSelect(index); }}
            >
              <span aria-hidden="true" />
            </button>
          );
        })}
      </div>
      {preview && previewCard && <aside
        className="cq-queue-minimap-preview"
        aria-hidden="true"
        style={{ left: preview.left }}
      >
        <small>{String(preview.index + 1).padStart(2, "0")} / {String(cards.length).padStart(2, "0")}</small>
        <strong>{previewCard.title}</strong>
        {(previewCard.sessionId ? summaries[previewCard.sessionId] : undefined) || (previewCard.excerpt !== previewCard.title ? previewCard.excerpt : undefined)
          ? <p>{(previewCard.sessionId ? summaries[previewCard.sessionId] : undefined) || previewCard.excerpt}</p>
          : null}
        <div className="cq-queue-minimap-environment">
          <span><WorkspaceMachineIcon name={previewCard.remote ? "remote" : "local"} size={14} />{previewCard.host}</span>
          <span><WorkspaceMachineIcon name="folder" size={14} />{previewCard.workspace}</span>
        </div>
      </aside>}
    </nav>
  );
}
