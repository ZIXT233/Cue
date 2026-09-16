#!/usr/bin/env node
/**
 * cursor 交互式启动耗时实测（真 pty）
 *
 * 直接运行：
 *     node bin/measure-cursor-boot.mjs
 *     node bin/measure-cursor-boot.mjs --repeat 3
 *     node bin/measure-cursor-boot.mjs --cwd D:\Cue\src-tauri
 *
 * 为什么必须用 pty：cursor 靠 isatty 判断进 TUI 还是 print 模式。
 * 用管道（stdio: pipe）测，它直接走 print 模式并报
 * "No prompt provided for print mode"，测到的不是真实启动路径。
 *
 * 依赖：node 内置无 pty。优先用 node-pty，没有则回落到 Windows 上的
 * winpty/conpty 直连（通过 Cue 已有的 portable-pty 不可用时，退化为
 * 用 `cmd /c start` + 日志文件的方式）。脚本会明确告诉你走的哪条路。
 */

const { spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1) return fallback;
  const v = process.argv[i + 1];
  return v && !v.startsWith('--') ? v : true;
}

const CWD = arg('cwd', 'D:\\Cue\\src-tauri');
const REPEAT = Number(arg('repeat', 2)) || 2;
const OBSERVE_MS = Number(arg('observe', 30000)) || 30000;

// cursor 安装位置：取最新的 versions\<date>-<hash>
function findCursor() {
  const base = path.join(os.homedir(), 'AppData', 'Local', 'cursor-agent');
  const versions = path.join(base, 'versions');
  if (!fs.existsSync(versions)) return null;
  const names = fs
    .readdirSync(versions)
    .filter((n) => /^\d{4}\.\d{1,2}\.\d{1,2}(-\d{2}-\d{2}-\d{2})?-[a-f0-9]+$/.test(n))
    .sort();
  for (const n of names.reverse()) {
    const dir = path.join(versions, n);
    const node = path.join(dir, 'node.exe');
    const script = path.join(dir, 'index.js');
    if (fs.existsSync(node) && fs.existsSync(script)) return { node, script, version: n };
  }
  return null;
}

// 尝试加载 node-pty
function loadPty() {
  const roots = [
    process.cwd(),
    __dirname,
    path.join(__dirname, '..'),
    'C:\\Users\\ZIXT\\.workbuddy\\binaries\\node\\workspace',
  ];
  for (const r of roots) {
    try {
      const m = require(path.join(r, 'node_modules', 'node-pty'));
      return m;
    } catch {}
  }
  try {
    return require('node-pty');
  } catch {}
  return null;
}

function stripAnsi(s) {
  return s
    .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '')
    .replace(/\x1b\][^\x07\x1b]*(\x07|\x1b\\)/g, '')
    .replace(/\x1b[()][A-Z0-9]/g, '')
    .replace(/\x1b[=>]/g, '')
    .replace(/[\x00-\x08\x0b-\x1f\x7f]/g, '');
}

async function runWithPty(pty, cursor, label) {
  return new Promise((resolve) => {
    const t0 = Date.now();
    const marks = [];
    let bytes = 0;
    let firstByteMs = null;
    let lastByteMs = null;
    const chunks = [];

    const term = pty.spawn(cursor.node, [cursor.script], {
      name: 'xterm-256color',
      cols: 100,
      rows: 30,
      cwd: CWD,
      env: { ...process.env, TERM: 'xterm-256color', COLORTERM: 'truecolor' },
    });

    term.onData((d) => {
      const at = Date.now() - t0;
      if (firstByteMs === null && d.length) firstByteMs = at;
      lastByteMs = at;
      bytes += d.length;
      if (marks.length < 40) marks.push(`${at}ms(+${d.length}B)`);
      if (chunks.length < 200) chunks.push(d);
    });

    // 判据：静止（N 秒无新字节）视为"启动完成，停在等输入"
    let lastSeen = -1;
    const idleTimer = setInterval(() => {
      if (firstByteMs === null) return;
      if (lastByteMs !== null && lastByteMs === lastSeen) {
        clearInterval(idleTimer);
        const idleSince = lastByteMs;
        try { term.kill(); } catch {}
        resolve({
          label,
          ok: true,
          firstByteMs,
          lastByteMs,
          idleAt: idleSince,
          bytes,
          marks,
          tail: stripAnsi(Buffer.concat(chunks.map((c) => Buffer.from(c, 'utf8'))).toString('utf8')).slice(0, 1200),
        });
      }
      lastSeen = lastByteMs;
    }, 500);

    setTimeout(() => {
      clearInterval(idleTimer);
      try { term.kill(); } catch {}
      if (firstByteMs === null) {
        resolve({ label, ok: false, reason: `${OBSERVE_MS}ms 内无任何输出`, bytes, marks, tail: '' });
      } else {
        resolve({
          label,
          ok: true,
          firstByteMs,
          lastByteMs,
          idleAt: lastByteMs,
          bytes,
          marks,
          tail: stripAnsi(Buffer.concat(chunks.map((c) => Buffer.from(c, 'utf8'))).toString('utf8')).slice(0, 1200),
          timedOut: true,
        });
      }
    }, OBSERVE_MS);
  });
}

// 无 node-pty 时的退化方案：Windows 上用 Cue 自身或 winpty 起 pty。
// 这里用 `winpty`（Git Bash 自带）若可用；否则明确报告无法测。
async function runNoPty() {
  const winpty = spawnSync('where', ['winpty'], { encoding: 'utf8', shell: true });
  if (winpty.status !== 0) return null;
  return 'winpty';
}

(async () => {
  const cursor = findCursor();
  const out = [];
  const say = (s) => { out.push(s); console.log(s); };

  say('=== cursor 交互式启动实测（真 pty） ===');
  say(`cwd      : ${CWD}`);
  say(`重复次数 : ${REPEAT}`);
  say('');

  if (!cursor) {
    say('找不到 cursor 安装目录，退出。');
    process.exit(1);
  }
  say(`cursor   : ${cursor.version}`);
  say(`node     : ${cursor.node}`);
  say(`script   : ${cursor.script}`);
  say('');

  const pty = loadPty();
  if (!pty) {
    say('!! 未找到 node-pty，无法建立真 pty。');
    say('   安装：');
    say('     cd C:\\Users\\ZIXT\\.workbuddy\\binaries\\node\\workspace');
    say('     ..\\versions\\22.22.2\\node.exe ..\\versions\\22.22.2\\node_modules\\npm\\bin\\npm-cli.js install node-pty');
    say('   然后再跑本脚本。');
    say('');
    say('（用管道测是无效的：cursor 会走 print 模式并报 No prompt provided。）');
    fs.writeFileSync(path.join(os.tmpdir(), 'cue-cursor-boot-measure.txt'), out.join('\n'), 'utf8');
    process.exit(2);
  }

  for (let i = 1; i <= REPEAT; i++) {
    const r = await runWithPty(pty, cursor, `run ${i}`);
    say(`--- run ${i} ---`);
    if (!r.ok) {
      say(`  !! ${r.reason}`);
    } else {
      say(`  首个字节      : ${r.firstByteMs} ms`);
      say(`  最后字节      : ${r.lastByteMs} ms`);
      say(`  静止(等输入)于: ${r.idleAt} ms   <-- 这就是"启动完成"`);
      say(`  总输出字节    : ${r.bytes}`);
    }
    say(`  节奏: ${r.marks.join(' | ')}`);
    if (r.tail) {
      say(`  净文本:`);
      say(`    ${JSON.stringify(r.tail.slice(0, 700))}`);
    }
    say('');
  }

  const dst = path.join(os.tmpdir(), 'cue-cursor-boot-measure.txt');
  fs.writeFileSync(dst, out.join('\n'), 'utf8');
  console.log(`结果已写入: ${dst}`);
})();
