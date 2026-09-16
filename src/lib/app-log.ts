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
