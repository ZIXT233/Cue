"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { harnessTerminalTheme, resolveTerminalThemeProfile, terminalThemeHostFromDocument, type TerminalThemeProfile } from "@/lib/terminal-theme";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { enhancedTerminalKey, decodeTerminalClipboard } from "@/lib/terminal-enhancements";
import { isFileDrag, droppedFiles, dropFilesError } from "@/lib/file-drop";
import { copyText } from "@/lib/clipboard";
import { Terminal } from "@xterm/xterm";
import { useI18n } from "@/hooks/useI18n";
import { createTerminalWriter, isTerminalAbortError, terminalRequest } from "@/lib/terminal-client";
import { setXtermProbe } from "@/lib/terminal-probe";
import { developerProbesEnabled } from "@/lib/developer-probes";
import { MAX_ATTACHED_IMAGE_BYTES, MAX_ATTACHED_IMAGES } from "@/lib/image-attachments";
import type { TerminalEvent } from "@/lib/terminal-manager";
import type { TerminalTab } from "./terminal-tab-state";

export type TerminalConnectionStatus = "connecting" | "ready" | "exited" | "error";
interface Props {
  onOutput?: (data: string) => void;
  onStatusChange?: (status: TerminalConnectionStatus) => void;
  embedded?: boolean;
  themeProfile?: TerminalThemeProfile;
  remote?: boolean;
  readOnly?: boolean;
  tab: TerminalTab;
  active: boolean;
  /** Reply to CSI ?1004h. I while the card is in the queue, O otherwise. */
  focusReporting?: boolean;
  inQueue?: boolean;
  onRestart: () => void;
  onClosed: () => void;
  onCloseError: () => void;
  onUnavailable?: () => void;
}

function liveThemeProfile(themeProfile: TerminalThemeProfile | undefined, remote: boolean | undefined): TerminalThemeProfile | undefined {
  if (typeof document === "undefined") return themeProfile === "grok" ? "grok" : undefined;
  return resolveTerminalThemeProfile(themeProfile, terminalThemeHostFromDocument(remote, document.documentElement, navigator));
}

export function TerminalPanel({ tab, active, onRestart, onClosed, onCloseError, onUnavailable, embedded = false, readOnly = false, onStatusChange, onOutput, themeProfile, remote = false, focusReporting = false, inQueue = false }: Props) {
  const { t } = useI18n();
  const { id, cwd, restored } = tab;
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const startRef = useRef<Promise<void>>(Promise.resolve());
  const writerRef = useRef<ReturnType<typeof createTerminalWriter> | null>(null);
  const callbacksRef = useRef({ onClosed, onCloseError, onOutput, onUnavailable });
  callbacksRef.current = { onClosed, onCloseError, onOutput, onUnavailable };
  const focusReportingRef = useRef(focusReporting);
  const focusArmedRef = useRef(false);
  const inQueueRef = useRef(inQueue);
  focusReportingRef.current = focusReporting;
  inQueueRef.current = inQueue;
  const sendFocusReport = useCallback((inQueueNow: boolean) => {
    if (!focusReportingRef.current || !focusArmedRef.current) return;
    writerRef.current?.write(inQueueNow ? "\x1b[I" : "\x1b[O");
  }, []);
  const [status, setStatus] = useState<"connecting" | "ready" | "exited" | "error">("connecting");
  const [error, setError] = useState<string | null>(null);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [reconnectKey, setReconnectKey] = useState(0);

  const searchRef = useRef<SearchAddon | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [regex, setRegex] = useState(false);
  const [searchInvalid, setSearchInvalid] = useState(false);
  const [matches, setMatches] = useState({ resultIndex: -1, resultCount: 0 });
  const [dragging, setDragging] = useState(false);
  const [clipboardPending, setClipboardPending] = useState<string | null>(null);
  const find = useCallback((previous = false, incremental = false) => {
    if (regex) {
      try { new RegExp(searchQuery); } catch { setSearchInvalid(true); searchRef.current?.clearDecorations(); return; }
    }
    setSearchInvalid(false);
    if (!searchQuery) { searchRef.current?.clearDecorations(); setMatches({ resultIndex: -1, resultCount: 0 }); return; }
    const options = { caseSensitive, regex, incremental, decorations: {
      matchBackground: "#796322", matchOverviewRuler: "#b69738",
      activeMatchBackground: "#35704d", activeMatchColorOverviewRuler: "#59c484",
    } };
    if (previous) searchRef.current?.findPrevious(searchQuery, options);
    else searchRef.current?.findNext(searchQuery, options);
  }, [caseSensitive, regex, searchQuery]);
  useEffect(() => {
    if (searchOpen) { searchInput.current?.focus(); find(false, true); }
    else searchRef.current?.clearDecorations();
  }, [searchOpen, find]);
  const closeSearch = () => { setSearchOpen(false); terminalRef.current?.focus(); };

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    focusArmedRef.current = false;
    let disposed = false;
    let events: EventSource | null = null;
    let offset: number | undefined;
    let connected = false;
    let exited = false;
    let inputFailed = false;
    let sessionReadOnly = readOnly;
    setStatus("connecting");
    setError(null);
    setExitCode(null);

    const conptyHost = !remote && typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
    let conptyCursorHidden = false;
    let conptyRevealTimer: ReturnType<typeof setTimeout> | undefined;
    const liveTheme = () => harnessTerminalTheme(document.documentElement.classList.contains("dark"), liveThemeProfile(themeProfile, remote));
    const terminal = new Terminal({
      cursorBlink: !conptyHost,
      allowProposedApi: true,
      fontFamily: getComputedStyle(container).getPropertyValue("--font-mono").trim() || "monospace",
      fontSize: 13,
      lineHeight: 1.25,
      scrollback: 8000,
      // xterm 6.0.0 + screenReaderMode re-sends the trailing character when an
      // IME commits in the middle of a line (xtermjs/xterm.js#5456 / PR #5698).
      // Re-enable after upgrading past that CompositionHelper fix.
      screenReaderMode: false,
      disableStdin: true,
      windowsPty: conptyHost ? { backend: "conpty", buildNumber: 26200 } : undefined,
      theme: liveTheme(),
    });
    const themeObserver = new MutationObserver(() => {
      terminal.options.theme = liveTheme();
    });
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "data-theme", "data-desktop-platform"] });
    terminalRef.current = terminal;
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(container);
    const hideConptyCursor = () => {
      if (!conptyHost) return;
      clearTimeout(conptyRevealTimer);
      if (!conptyCursorHidden) {
        conptyCursorHidden = true;
        container.classList.add("is-conpty-redraw");
      }
      conptyRevealTimer = setTimeout(() => {
        conptyCursorHidden = false;
        if (!disposed) container.classList.remove("is-conpty-redraw");
      }, 40);
    };
    // Let native horizontal gestures reach the card deck without xterm turning
    // them into terminal input or cancelling them. Vertical terminal scrolling
    // and detached terminals keep their existing behavior.
    const preserveDeckGesture = (event: WheelEvent) => {
      if (Math.abs(event.deltaX) > Math.abs(event.deltaY) && container.closest(".cq-deck-scroller")) {
        event.stopPropagation();
      }
    };
    container.addEventListener("wheel", preserveDeckGesture, { capture: true, passive: true });
    const search = new SearchAddon();
    terminal.loadAddon(search);
    searchRef.current = search;
    const searchResults = search.onDidChangeResults(setMatches);
    let replaying = true;
    let outputQueue = Promise.resolve();
    const clipboard = terminal.parser.registerOscHandler(52, (payload) => {
      if (replaying || disposed || !document.hasFocus() || !container.contains(document.activeElement)) return true;
      const text = decodeTerminalClipboard(payload);
      if (text !== null) void copyText(text).catch(() => { if (!disposed) setClipboardPending(text); });
      return true;
    });
    // WebGL draws the caret into the canvas, so ConPTY cursor hiding needs DOM.
    let gpu: import("@xterm/addon-webgl").WebglAddon | undefined;
    let gpuLoss: { dispose(): void } | undefined;
    if (!conptyHost) {
      void import("@xterm/addon-webgl").then(({ WebglAddon }) => {
        if (disposed) return;
        const addon = new WebglAddon();
        try {
          terminal.loadAddon(addon);
          gpu = addon;
          gpuLoss = addon.onContextLoss(() => {
            gpuLoss?.dispose(); gpuLoss = undefined;
            gpu?.dispose(); gpu = undefined;
            terminal.refresh(0, terminal.rows - 1);
          });
          terminal.refresh(0, terminal.rows - 1);
        } catch { addon.dispose(); }
      }).catch(() => { /* DOM rendering remains available. */ });
    }
    const writer = createTerminalWriter(id, (reason) => {
      if (disposed) return;
      inputFailed = true;
      terminal.options.disableStdin = true;
      setError(reason.message);
      setStatus("error");
    });
    writerRef.current = writer;
    let pendingInput = "";
    let inputRaf = 0;
    const flushInput = () => {
      inputRaf = 0;
      const data = pendingInput;
      pendingInput = "";
      if (data && connected && !exited && !inputFailed && !sessionReadOnly) writer.write(data);
    };
    const sendInput = (data: string) => {
      if (!connected || exited || inputFailed || sessionReadOnly || terminal.options.disableStdin) return;
      if (!conptyHost) { writer.write(data); return; }
      // One HTTP POST per animation frame instead of one per keystroke.
      pendingInput += data;
      if (!inputRaf) inputRaf = requestAnimationFrame(flushInput);
    };
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown" || event.isComposing || event.keyCode === 229) return true;
      const mac = /Mac|iPhone|iPad/.test(navigator.platform);
      const key = event.key.toLowerCase();
      if ((mac ? event.metaKey : event.ctrlKey) && key === "f") {
        event.preventDefault(); event.stopPropagation(); setSearchOpen(true);
        requestAnimationFrame(() => { searchInput.current?.focus(); searchInput.current?.select(); });
        return false;
      }
      if ((event.ctrlKey || event.metaKey) && key === "v") return false;
      if ((event.ctrlKey || event.metaKey) && key === "c" && terminal.hasSelection()) return false;
      const data = enhancedTerminalKey(event, mac);
      if (data !== null) {
        event.preventDefault(); event.stopPropagation();
        sendInput(data);
        return false;
      }
      return true;
    });
    const paste = (event: ClipboardEvent) => {
      const files = Array.from(event.clipboardData?.items ?? [])
        .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
        .map((item) => item.getAsFile()).filter((file): file is File => file !== null);
      if (!files.length) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!connected || exited || inputFailed || sessionReadOnly || terminal.options.disableStdin) return;
      if (files.length > MAX_ATTACHED_IMAGES || files.some((file) => file.size > MAX_ATTACHED_IMAGE_BYTES)) {
        setError("Paste up to 10 images, each 10 MB or smaller.");
        return;
      }
      setError(null);
      if (inputRaf) { cancelAnimationFrame(inputRaf); flushInput(); }
      writer.pasteImages(files, terminal.modes.bracketedPasteMode);
    };
    // Capture before xterm's text-only paste listener consumes the clipboard.
    container.addEventListener("paste", paste, true);
    const dragOver = (event: DragEvent) => {
      if (!event.dataTransfer || !isFileDrag(event.dataTransfer)) return;
      event.preventDefault(); event.stopPropagation();
      const writable = connected && !exited && !inputFailed && !sessionReadOnly && !terminal.options.disableStdin;
      event.dataTransfer.dropEffect = writable ? "copy" : "none";
      setDragging(writable);
    };
    const dragLeave = (event: DragEvent) => {
      if (!(event.relatedTarget instanceof Node) || !container.contains(event.relatedTarget)) setDragging(false);
    };
    const drop = (event: DragEvent) => {
      if (!event.dataTransfer || !isFileDrag(event.dataTransfer)) return;
      event.preventDefault(); event.stopPropagation(); setDragging(false);
      if (!connected || exited || inputFailed || sessionReadOnly || terminal.options.disableStdin) return;
      try {
        const files = droppedFiles(event.dataTransfer);
        const reason = dropFilesError(files);
        if (reason) throw new Error(reason);
        if (files.length) {
          setError(null);
          if (inputRaf) { cancelAnimationFrame(inputRaf); flushInput(); }
          writer.pasteFiles(files, terminal.modes.bracketedPasteMode);
          terminal.focus();
        }
      } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    };
    container.addEventListener("dragover", dragOver);
    container.addEventListener("dragleave", dragLeave);
    container.addEventListener("drop", drop);
    const onData = terminal.onData((data) => {
      sendInput(data);
    });
    const fitAndResize = () => {
      if (!container.offsetWidth || !container.offsetHeight) return;
      fit.fit();
    };
    const onResize = terminal.onResize(({ cols, rows }) => {
      if (connected && !exited && !inputFailed && !sessionReadOnly) writer.resize(cols, rows);
    });
    const resizeObserver = new ResizeObserver(fitAndResize);
    resizeObserver.observe(container);

    let sseMessages = 0;
    let bytesWritten = 0;
    let skipped = 0;
    const publishProbe = (statusName: string) => {
      if (!developerProbesEnabled()) return;
      setXtermProbe(id, {
        cols: terminal.cols,
        rows: terminal.rows,
        sseMessages,
        bytesWritten,
        lastOffset: offset,
        lastReset: false,
        status: `${statusName}${skipped ? ` skip=${skipped}` : ""}`,
      });
    };
    const connect = () => {
      if (disposed || exited || !navigator.onLine) return;
      events?.close();
      events = new EventSource(`/api/terminal/${encodeURIComponent(id)}/events${offset === undefined ? "" : `?after=${offset}`}`);
      events.onmessage = (message) => {
        const event = JSON.parse(message.data) as TerminalEvent;
        sseMessages += 1;
        if (event.type === "output") {
          if (!event.reset && offset !== undefined && event.offset <= offset) {
            skipped += 1;
            publishProbe("ready");
            return;
          }
          hideConptyCursor();
          if (focusReportingRef.current && event.data.includes("\x1b[?1004h")) {
            focusArmedRef.current = true;
            sendFocusReport(inQueueRef.current);
          }
          if (event.data.includes("\x1b[?1004l")) focusArmedRef.current = false;
          bytesWritten += event.data.length;
          outputQueue = outputQueue.then(() => new Promise<void>((resolve) => {
            if (disposed) { resolve(); return; }
            replaying = event.reset === true;
            if (event.reset) terminal.reset();
            terminal.write(event.data, resolve);
          }));
          callbacksRef.current.onOutput?.(event.data);
          offset = event.offset;
          publishProbe("ready");
        } else {
          exited = true;
          connected = false;
          terminal.options.disableStdin = true;
          events?.close();
          setExitCode(event.type === "exit" ? event.exitCode : null);
          setStatus("exited");
        }
      };
      events.onopen = () => {
        connected = true;
        if (inputFailed) return;
        terminal.options.disableStdin = sessionReadOnly;
        setStatus("ready");
        fitAndResize();
        if (!sessionReadOnly) writer.resize(terminal.cols, terminal.rows);
        if (container.offsetWidth && container.offsetHeight) terminal.focus();
        publishProbe("ready");
      };
      events.onerror = () => {
        if (disposed || exited) return;
        connected = false;
        terminal.options.disableStdin = true;
        setStatus(events?.readyState === EventSource.CLOSED ? "error" : "connecting");
      };
    };

    startRef.current = (async () => {
      fitAndResize();
      if (restored || reconnectKey > 0) {
        // Restoring a tab must never silently launch a replacement shell.
        const info = await terminalRequest(`/api/terminal/${encodeURIComponent(id)}`);
        sessionReadOnly ||= info.readOnly === true;
      } else {
        await terminalRequest("/api/terminal", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ id, cwd, cols: terminal.cols, rows: terminal.rows }),
        });
      }
      connect();
    })().catch((reason: Error) => {
      if (disposed || isTerminalAbortError(reason)) return;
      if (restored) {
        callbacksRef.current.onUnavailable?.();
        return;
      }
      setError(reason.message);
      setStatus("error");
    });

    const pageHide = () => {
      connected = false;
      terminal.options.disableStdin = true;
      events?.close();
      if (!exited && !inputFailed) setStatus("connecting");
    };
    const pageShow = (event: PageTransitionEvent) => { if (event.persisted) connect(); };
    window.addEventListener("pagehide", pageHide);
    window.addEventListener("pageshow", pageShow);
    window.addEventListener("offline", pageHide);
    window.addEventListener("online", connect);
    return () => {
      disposed = true;
      if (focusReportingRef.current && focusArmedRef.current && !inQueueRef.current) writer.write("\x1b[O");
      clearTimeout(conptyRevealTimer);
      if (inputRaf) cancelAnimationFrame(inputRaf);
      if (pendingInput) writer.write(pendingInput);
      pendingInput = "";
      events?.close();
      void writer.stop();
      resizeObserver.disconnect();
      themeObserver.disconnect();
      container.removeEventListener("paste", paste, true);
      container.removeEventListener("dragover", dragOver);
      container.removeEventListener("dragleave", dragLeave);
      container.removeEventListener("drop", drop);
      clipboard.dispose();
      searchResults.dispose();
      searchRef.current = null;
      onData.dispose();
      onResize.dispose();
      window.removeEventListener("pagehide", pageHide);
      window.removeEventListener("pageshow", pageShow);
      window.removeEventListener("offline", pageHide);
      window.removeEventListener("online", connect);
      gpuLoss?.dispose();
      gpu?.dispose();
      container.removeEventListener("wheel", preserveDeckGesture, true);
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [id, cwd, restored, reconnectKey, readOnly, themeProfile, remote]);

  useEffect(() => { onStatusChange?.(status); }, [status, onStatusChange]);

  useEffect(() => {
    if (active) terminalRef.current?.focus();
  }, [active]);

  useEffect(() => {
    sendFocusReport(inQueue);
  }, [inQueue, sendFocusReport]);

  useEffect(() => {
    if (!tab.closing) return;
    let cancelled = false;
    if (terminalRef.current) terminalRef.current.options.disableStdin = true;
    void (async () => {
      try { await startRef.current; } catch { /* Start already failed; still drop the tab. */ }
      await writerRef.current?.stop();
      try {
        await terminalRequest(`/api/terminal/${encodeURIComponent(id)}`, { method: "DELETE", keepalive: true });
      } catch (reason) {
        if (!isTerminalAbortError(reason)) throw reason;
      }
      if (!cancelled) callbacksRef.current.onClosed();
    })().catch((reason: Error) => {
      if (cancelled) return;
      if (isTerminalAbortError(reason)) {
        callbacksRef.current.onClosed();
        return;
      }
      setError(reason.message);
      setStatus("error");
      callbacksRef.current.onCloseError();
    });
    return () => { cancelled = true; };
  }, [id, tab.closing]);

  return (
    <section className={`terminal-panel${dragging ? " terminal-file-drag" : ""}${embedded ? " terminal-panel-embedded" : ""}`} data-terminal-theme={liveThemeProfile(themeProfile, remote)} suppressHydrationWarning aria-label={t("terminal.title")}>
      {!embedded && <header className="terminal-panel-header">
        <div className="terminal-panel-path">
          <span className={`terminal-status-dot is-${status}`} title={t(`terminal.${status}`)} />
          <span title={cwd}>{cwd}</span>
        </div>
        {status === "error" && (
          <button type="button" onClick={() => setReconnectKey((key) => key + 1)} disabled={Boolean(tab.closing)} title={t("terminal.reconnect")} aria-label={t("terminal.reconnect")}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-2 2M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l2-2" />
            </svg>
          </button>
        )}
        <button type="button" onClick={onRestart} disabled={Boolean(tab.closing)} title={t("terminal.restart")} aria-label={t("terminal.restart")}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M20 11a8 8 0 1 0-2.34 5.66" /><polyline points="20 4 20 11 13 11" />
          </svg>
        </button>
      </header>}
      <button type="button" className="terminal-find-toggle" onClick={() => setSearchOpen(true)} title={t("terminal.find")} aria-label={t("terminal.find")}><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><circle cx="10" cy="10" r="6" /><path d="m15 15 5 5" /></svg></button>
      {searchOpen && <div className="terminal-find" role="search" onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape") { event.preventDefault(); closeSearch(); }
        if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); find(event.shiftKey); }
      }}>
        <input ref={searchInput} aria-label={t("terminal.find")} placeholder={t("terminal.find")} value={searchQuery} aria-invalid={searchInvalid} onChange={(event) => setSearchQuery(event.target.value)} />
        <span role="status">{searchInvalid ? t("terminal.invalidRegex") : `${matches.resultIndex + 1}/${matches.resultCount}`}</span>
        <button type="button" aria-label={t("terminal.matchCase")} title={t("terminal.matchCase")} aria-pressed={caseSensitive} onClick={() => setCaseSensitive(value => !value)}>Aa</button>
        <button type="button" aria-label={t("terminal.regex")} title={t("terminal.regex")} aria-pressed={regex} onClick={() => setRegex(value => !value)}>.*</button>
        <button type="button" aria-label={t("terminal.previousMatch")} title={t("terminal.previousMatch")} onClick={() => find(true)}>↑</button>
        <button type="button" aria-label={t("terminal.nextMatch")} title={t("terminal.nextMatch")} onClick={() => find()}>↓</button>
        <button type="button" aria-label={t("files.cancel")} onClick={closeSearch}>×</button>
      </div>}
      {dragging && <div className="terminal-drop-hint">{t("terminal.dropFiles")}</div>}
      <div className="terminal-panel-messages">
        {clipboardPending !== null && <button type="button" onClick={() => void copyText(clipboardPending).then(() => setClipboardPending(null)).catch(() => setError(t("terminal.copyFailed")))}>{t("terminal.copyRemote")}</button>}
        {error && <div className="terminal-panel-error" role="alert">
          <span>{error.includes(" ") ? error : t(error)}</span>
          <button type="button" onClick={() => { setError(null); setReconnectKey((key) => key + 1); }} disabled={Boolean(tab.closing)}>{t("terminal.reconnect")}</button>
        </div>}
        {!embedded && status === "exited" && <div className="terminal-panel-exit" role="status">{exitCode === null ? t("terminal.exited") : t("terminal.exitCode", { code: exitCode })}</div>}
      </div>
      <div className="terminal-xterm"><div ref={containerRef} className="terminal-xterm-host" /></div>
    </section>
  );
}
