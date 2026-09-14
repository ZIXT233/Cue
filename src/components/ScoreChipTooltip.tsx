"use client";
import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

export function ScoreChipTooltip({ children, text }: { children: React.ReactNode; text: React.ReactNode }) {
  const id = useId();
  const anchor = useRef<HTMLSpanElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);
  useEffect(() => () => clearTimeout(timer.current), []);
  const show = () => {
    clearTimeout(timer.current);
    const rect = anchor.current?.getBoundingClientRect();
    if (!rect) return;
    setPosition({
      left: Math.max(8, Math.min(rect.left, window.innerWidth - 368)),
      top: Math.max(8, Math.min(rect.bottom + 6, window.innerHeight - 196)),
    });
  };
  const hide = () => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setPosition(null), 120);
  };
  return <>
    <span ref={anchor} className="cq-score-tooltip-anchor" tabIndex={0} aria-describedby={position ? id : undefined} onMouseEnter={show} onMouseLeave={hide} onFocus={show} onBlur={hide} onClick={show} onKeyDown={event => { if (event.key === "Escape") setPosition(null); }}>{children}</span>
    {position && createPortal(<div id={id} className="cq-score-tooltip" role="tooltip" style={position} onMouseEnter={() => clearTimeout(timer.current)} onMouseLeave={hide}>{text}</div>, document.body)}
  </>;
}
