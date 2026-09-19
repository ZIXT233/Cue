"use client";

import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

/** Keep nested chip portals inside the hover/focus lifecycle of the formula. */
export function ScoreFormulaPopover({ label, children }: { label: ReactNode; children: ReactNode }) {
  const id = useId();
  const anchor = useRef<HTMLButtonElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const focused = useRef(false);
  const [position, setPosition] = useState<{ right: number; top: number } | null>(null);
  const keepOpen = () => clearTimeout(timer.current);
  const close = () => { keepOpen(); setPosition(null); };
  const show = () => {
    keepOpen();
    const rect = anchor.current?.getBoundingClientRect();
    if (rect) setPosition({ right: Math.max(8, window.innerWidth - rect.right), top: rect.bottom + 6 });
  };
  const leave = () => {
    keepOpen();
    timer.current = setTimeout(() => { if (!focused.current) setPosition(null); }, 220);
  };
  useEffect(() => () => clearTimeout(timer.current), []);
  useEffect(() => {
    if (!position) return;
    const dismiss = () => setPosition(null);
    window.addEventListener("resize", dismiss);
    return () => window.removeEventListener("resize", dismiss);
  }, [position]);
  return <span className="cq-score-menu" onMouseEnter={keepOpen} onMouseLeave={leave}
    onFocusCapture={() => { focused.current = true; keepOpen(); }}
    onBlurCapture={() => { focused.current = false; leave(); }}
    onKeyDown={event => { if (event.key === "Escape") { event.stopPropagation(); close(); } }}>
    <button ref={anchor} type="button" className="cq-score" aria-haspopup="dialog" aria-expanded={!!position}
      aria-controls={position ? id : undefined} onMouseEnter={show} onFocus={show} onClick={show}>{label}</button>
    {position && createPortal(<div id={id} className="cq-score-formula-popover" role="dialog" aria-labelledby={id + "-total"}
      style={{ ...position, maxWidth: `calc(100vw - ${position.right + 8}px)` }} onMouseEnter={keepOpen}>
      <span id={id + "-total"} className="cq-score-formula-total">{label}</span>
      {children}
    </div>, document.body)}
  </span>;
}
