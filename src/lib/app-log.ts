export type AppLogLevel = "error" | "warn" | "info" | "debug" | "trace";

export function appLog(level: AppLogLevel, sys: string, msg: string, ids?: { card?: string; term?: string }) {
  void fetch("/api/logs", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ level, sys, msg, card: ids?.card, term: ids?.term }),
    keepalive: true,
  }).catch(() => {});
}

export function installFrontendLogBridge() {
  const original = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    original(...args);
    const msg = args.map((item) => (item instanceof Error ? item.stack || item.message : typeof item === "string" ? item : JSON.stringify(item))).join(" ");
    if (msg) appLog("error", "ui", msg.slice(0, 2000));
  };
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason instanceof Error ? event.reason.stack || event.reason.message : String(event.reason ?? "unhandledrejection");
    appLog("error", "ui", reason.slice(0, 2000));
  });
}

/// OSC 10/11 color-query experiment: hex-dump every chunk that carries an
/// OSC 4/10/11/12 sequence or an `rgb:` reply, stamped with an epoch-ms clock
/// that matches the backend's `osc` lines. The point is to trace the query and
/// its reply hop by hop — SSE in → xterm onData → POST → Rust write → SSH —
/// and see where the bytes get lost, split or duplicated. Everything else
/// (keystrokes, ordinary output) stays silent.
///
/// Dumps UTF-16 code units, not raw bytes: fine here because the sequences of
/// interest (ESC ] 10 ; rgb:… ESC \) are pure ASCII.
const OSC_COLOR_PROBE = /\x1b\](?:4|10|11|12);/;
export function oscTrace(kind: string, data: string, ids?: { card?: string; term?: string }) {
  if (!OSC_COLOR_PROBE.test(data) && !data.includes("rgb:")) return;
  const shown: string[] = [];
  for (let i = 0; i < data.length && shown.length < 512; i += 1) shown.push(data.charCodeAt(i).toString(16).padStart(2, "0"));
  const rest = data.length - shown.length;
  appLog("debug", "osc", `${kind} t=${(performance.timeOrigin + performance.now()).toFixed(1)} len=${data.length} hex=${shown.join(" ")}${rest > 0 ? ` …(+${rest}B)` : ""}`, ids);
}
