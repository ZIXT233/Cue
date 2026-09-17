// Is cursor's startup delay actually caused by the network?
//
// The claim under test: cursor blocks at boot on remote config (Statsig feature
// gates + `cli-config.json` resolution) before it reaches its first hook. The
// evidence so far is one run where statsig-cache.json was rewritten 32s after
// spawn and cli-config.json 33s after that. One sample is a correlation, and
// the two halves are suspiciously equal, so this script tries to break the
// claim rather than confirm it.
//
// It measures spawn -> first-byte only. It does NOT measure "-> hook", because
// a hook needs a real interactive session and a real prompt; that leg comes from
// Que's own logs (`first hook after Nms`). This script answers one narrow
// question: does the CLI's own bootstrap change when the network does?
//
//   node bin/probe-cursor-boot.mjs
//
// Runs three conditions:
//   warm  - plain run, whatever cache state exists now
//   cold  - statsig-cache.json temporarily moved aside
//   off   - proxy env pointed at a dead port, so any outbound HTTPS hangs/fails
//
// If the delay is network-bound, `off` should be visibly different from `warm`.
// If all three look the same, the delay is local work and the Statsig theory is
// wrong — which is a perfectly good outcome and the reason this exists.

import { existsSync, readdirSync, renameSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const CURSOR_HOME = join(homedir(), ".cursor");
const CACHE = join(CURSOR_HOME, "statsig-cache.json");
const BACKUP = `${CACHE}.probe-backup`;

function newestCursor() {
  const root = join(process.env.LOCALAPPDATA ?? "", "cursor-agent", "versions");
  if (!existsSync(root)) return null;
  const versions = readdirSync(root).filter((v) => {
    try {
      return statSync(join(root, v)).isDirectory();
    } catch {
      return false;
    }
  });
  if (!versions.length) return null;
  // Versions sort lexically as dates in cursor's scheme (2026.09.10-<hash>).
  const latest = versions.sort().at(-1);
  const node = join(root, latest, "node.exe");
  const script = join(root, latest, "index.js");
  if (!existsSync(node) || !existsSync(script)) return null;
  return { version: latest, node, script, dir: join(root, latest) };
}

// A pty is mandatory: with a pipe, cursor detects a non-tty and enters print
// mode, which skips the entire interactive bootstrap this script exists to
// measure. node-pty is the only reliable way to get one from node.
async function loadPty() {
  try {
    return await import("node-pty");
  } catch {
    return null;
  }
}

async function once(pt, target, label, extraEnv) {
  return new Promise((resolve) => {
    const started = Date.now();
    let firstByte = null;
    let bytes = 0;

    const pty = pt.spawn(target.node, [target.script], {
      name: "xterm-256color",
      cols: 100,
      rows: 30,
      cwd: join(process.cwd(), "src-tauri"),
      env: {
        ...process.env,
        CURSOR_INVOKED_AS: "cursor-agent",
        ...extraEnv,
      },
    });

    const timer = setTimeout(() => {
      try {
        pty.kill();
      } catch {}
      resolve({ label, firstByte, bytes, ms: Date.now() - started, timedOut: true });
    }, 120_000);

    pty.onData((chunk) => {
      bytes += chunk.length;
      if (firstByte === null) firstByte = Date.now() - started;
    });

    pty.onExit(() => {
      clearTimeout(timer);
      resolve({ label, firstByte, bytes, ms: Date.now() - started, timedOut: false });
    });
  });
}

const target = newestCursor();
if (!target) {
  console.error("找不到 cursor-agent 安装目录。");
  console.error(`预期位置: ${join(process.env.LOCALAPPDATA ?? "<LOCALAPPDATA>", "cursor-agent", "versions")}`);
  process.exit(1);
}

const pt = await loadPty();
if (!pt) {
  console.error("缺少 node-pty——这个脚本必须要一个真正的 pty。");
  console.error("装法: npm install node-pty");
  console.error("");
  console.error("为什么不能用管道代替: cursor 检测到 stdin 不是 tty 会进入 print 模式，");
  console.error("那条路径跳过了整个交互式自举，测出来的数字跟真实启动无关。");
  process.exit(2);
}

console.log(`cursor-agent ${target.version}`);
console.log(`  node:   ${target.node}`);
console.log(`  script: ${target.script}`);
console.log("");

// The proxy trick: point HTTPS at a port nothing listens on. Any blocking
// outbound call then fails fast (ECONNREFUSED) instead of succeeding. This is
// the control that separates "waits for network" from "does local work".
const DEAD_PROXY = "http://127.0.0.1:1";

const conditions = [
  ["warm", "热缓存（现状）", {}],
  ["off", "断网（代理指向死端口）", { HTTPS_PROXY: DEAD_PROXY, HTTP_PROXY: DEAD_PROXY }],
];

const results = [];

for (const [id, desc, env] of conditions) {
  if (id === "cold") continue;
  process.stdout.write(`跑 ${id.padEnd(5)} ${desc} ... `);
  const r = await once(pt, target, id, env);
  results.push(r);
  console.log(
    r.firstByte === null
      ? `没吐任何字节（${r.ms}ms${r.timedOut ? " 超时" : " 退出"}）`
      : `首字节 ${r.firstByte}ms，共 ${r.bytes}B，${r.ms}ms${r.timedOut ? " 超时" : ""}`
  );
}

// Cold cache: move the cache aside so cursor must refetch from scratch. Done
// last so a crash mid-run cannot leave the user without their cache.
if (existsSync(CACHE)) {
  renameSync(CACHE, BACKUP);
  try {
    process.stdout.write(`跑 cold  冷缓存（缓存已移走） ... `);
    const r = await once(pt, target, "cold", {});
    results.push(r);
    console.log(
      r.firstByte === null
        ? `没吐任何字节（${r.ms}ms${r.timedOut ? " 超时" : " 退出"}）`
        : `首字节 ${r.firstByte}ms，共 ${r.bytes}B，${r.ms}ms${r.timedOut ? " 超时" : ""}`
    );
  } finally {
    // Always restore, even if the run threw.
    try {
      renameSync(BACKUP, CACHE);
      console.log("（缓存已还原）");
    } catch (e) {
      console.error(`!! 还原缓存失败，备份在 ${BACKUP}，请手动改名回 statsig-cache.json`);
    }
  }
} else {
  console.log("跳过 cold：没有找到 statsig-cache.json");
}

console.log("");
console.log("=== 结果 ===");
for (const r of results) {
  const fb = r.firstByte === null ? "(无输出)" : `${r.firstByte}ms`;
  console.log(`${r.label.padEnd(5)} 首字节 ${fb.padEnd(10)} 字节数 ${String(r.bytes).padStart(7)}`);
}

// Read the spread, not the individual numbers: one slow run proves nothing on
// its own, but "off" being consistently faster than "warm" would mean the
// network call is on the critical path and failing fast beats waiting.
const seen = results.filter((r) => r.firstByte !== null);
if (seen.length >= 2) {
  const values = seen.map((r) => r.firstByte);
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  console.log("");
  console.log(`首字节区间 ${lo}ms – ${hi}ms（差 ${hi - lo}ms）`);
  if (hi - lo < 300) {
    console.log("三种条件几乎一样 → 启动开销是本地计算，不是网络。Statsig 假设不成立。");
  } else {
    console.log("条件之间有可见差异 → 网络确实在关键路径上，看哪个条件最快。");
  }
}
