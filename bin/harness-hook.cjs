/* eslint-disable @typescript-eslint/no-require-imports -- Standalone passive CLI hook. */
// Shared ingress for built-in CLI adapters. Public contract: docs/harness/hook-api.md
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { randomUUID } = require('node:crypto');
const explicitEvent = process.argv[2];
const cursorEvents = new Set(['sessionStart', 'beforeSubmitPrompt', 'preToolUse', 'postToolUse', 'postToolUseFailure', 'beforeShellExecution', 'beforeMCPExecution', 'afterAgentResponse', 'stop', 'sessionEnd']);
function cursorReply(event) {
  if (event === 'beforeSubmitPrompt') return { continue: true };
  if (event === 'preToolUse' || event === 'beforeShellExecution' || event === 'beforeMCPExecution') return { permission: 'allow' };
  return {};
}
let inferredKind = undefined;
const parts = __dirname.split(path.sep);
const hpIdx = parts.lastIndexOf('harness-plugins');
if (hpIdx >= 0 && parts[hpIdx + 1]) {
  inferredKind = parts[hpIdx + 1];
}
const kind = process.env.QUE_HARNESS_KIND || inferredKind || (cursorEvents.has(explicitEvent) ? 'cursor' : undefined);
const token = process.env.QUE_HARNESS_CHANNEL;
const envDirectory = process.env.QUE_HARNESS_SIGNAL_DIR;
const activePath = path.join(__dirname, 'active.json');
// User-level hooks fire for IDE chats, external terminals, etc., which inherit neither
// SIGNAL_DIR nor CHANNEL. Those sessions are not queue cards, so their events go to the
// external notification sink instead of being dropped — the queue surfaces them as a
// transient, never-persisted notice.
const externalPath = !envDirectory && !token ? externalDirectory() : undefined;
// Cursor observe hooks ignore stdout, but answering before stdin is fully read
// lets the worker tear the process down before replyPreview is written. Reply
// after the signal (beforeSubmitPrompt still returns continue:true).
if (kind === 'gemini' || kind === 'grok') process.stdout.write('{}\n');
const knownHarnesses = new Set(['cursor', 'codex', 'antigravity', 'gemini', 'grok', 'claude', 'opencode', 'codebuddy', 'pi', 'omp']);
if (!envDirectory && !token && !legacyActiveDirectory() && (!kind || !knownHarnesses.has(kind))) {
  process.exit(0);
}
const at = Date.now();
let input = '', oversized = false, done = false, finished = false;
// CLI hook runners kill us on their own deadline (codex 5s, cursor 15s). Fire
// before theirs so a hung stdin still delivers the signal instead of dying
// with a "hook timed out" and losing the event.
const watchdog = Math.max(500, Number(process.env.QUE_HARNESS_WATCHDOG_MS) || 8000);
const timer = setTimeout(() => { consume(); finish(); }, watchdog);
function finish() {
  if (finished) return;
  finished = true;
  clearTimeout(timer);
  if (kind === 'cursor') process.stdout.write(JSON.stringify(cursorReply(explicitEvent)) + '\n');
  process.exit(0);
}
process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => {
  if (input.length + chunk.length > 1024 * 1024) { oversized = true; input = ''; finish(); return; }
  input += chunk;
  consume();
});
process.stdin.on('error', () => finish());
process.stdin.on('end', () => { consume(); finish(); });

function readActive() {
  try { return JSON.parse(fs.readFileSync(activePath, 'utf8')); }
  catch { return undefined; }
}

function legacyActiveDirectory() {
  const active = readActive();
  return typeof active?.directory === 'string' && active.directory ? active.directory : undefined;
}

// Where a session Que never launched parks its events. The plugin lives at
// <data>/harness-plugins/<kind>/hook.cjs, so the data root is the parent of the
// plugin root; the bin/ copy used by dev and tests falls back to the default
// install location.
function externalDirectory() {
  if (process.env.QUE_EXTERNAL_SIGNAL_DIR) return process.env.QUE_EXTERNAL_SIGNAL_DIR;
  const marker = `${path.sep}harness-plugins${path.sep}`;
  const index = __dirname.lastIndexOf(marker);
  if (index > 0) return path.join(__dirname.slice(0, index), 'external-signals');
  return path.join(os.homedir(), '.que', 'external-signals');
}

// External sinks have no reader while Que is closed, so drop stale files here.
function pruneExternal(directory) {
  try {
    const cutoff = Date.now() - 5 * 60 * 1000;
    for (const name of fs.readdirSync(directory)) {
      if (!/^\d+-[a-f0-9-]+\.json$/.test(name)) continue;
      const target = path.join(directory, name);
      try { if (fs.statSync(target).mtimeMs < cutoff) fs.unlinkSync(target); } catch { /* Best effort. */ }
    }
  } catch { /* The sink is optional. */ }
}

function workspaceRootOf(payload) {
  const roots = payload.workspace_roots ?? payload.workspaceRoots ?? payload.workspace_root ?? payload.cwd;
  const value = Array.isArray(roots) ? roots[0] : roots;
  return typeof value === 'string' ? value.replace(/[\x00-\x1f\x7f]/g, ' ').trim().slice(0, 512) || undefined : undefined;
}

function replyText(payload) {
  return payload.text ?? payload.last_assistant_message ?? payload.lastAssistantMessage
    ?? payload.prompt_response ?? payload.response ?? payload.message ?? payload.content;
}

function consume() {
  if (done || oversized) return;
  let payload;
  try { payload = JSON.parse(input.replace(/^\uFEFF/, '')); }
  catch { return; }
  done = true;
  try {
    let eventName = explicitEvent || payload.hook_event_name || ({session_start:"SessionStart",user_prompt_submit:"UserPromptSubmit",pre_tool_use:"PreToolUse",post_tool_use:"PostToolUse",post_tool_use_failure:"PostToolUseFailure",stop_cancelled:"StopCancelled",stop:"Stop",stop_failure:"StopFailure",notification:"Notification"})[payload.hookEventName];
    // The ingress maps names, it never renames an event into a different meaning: a
    // harness's own vocabulary is passed through and the state machine reads it. A fact
    // the contract has no name for (agy's "this Stop is not a turn end") travels as a
    // field, so a reader can always see what was really reported.
    const fullyIdle = payload.fullyIdle ?? payload.fully_idle;
    const text = value => typeof value === 'string' ? value.replace(/[\x00-\x1f\x7f]/g, ' ').trim().slice(0, 160) : undefined;
    const directory = envDirectory || externalPath || (kind === 'cursor' ? undefined : legacyActiveDirectory());
    const external = externalPath !== undefined && directory === externalPath;
    // A card preview only has to hint at the reply; an external notice is the only
    // place that reply will ever be read, so keep its line breaks and its length.
    const externalReply = value => typeof value === 'string'
      ? value.replace(/\r\n?/g, '\n').replace(/[^\S\n]+/g, ' ').replace(/[\x00-\x08\x0b-\x1f\x7f]/g, '').trim().slice(0, 2000)
      : undefined;
    const completion = eventName === 'afterAgentResponse' || ['Stop', 'stop', 'AfterAgent'].includes(eventName);
    const event = { kind, at, event: eventName, fullyIdle: typeof fullyIdle === 'boolean' ? fullyIdle : undefined, replyPreview: completion ? (external ? externalReply(replyText(payload)) : text(replyText(payload))) : undefined, sessionId: payload.conversationId || payload.conversation_id || payload.session_id || payload.sessionId,
      agentId: payload.agent_id || payload.agentId, tool: payload.toolCall?.name ?? payload.tool_name ?? payload.toolName ?? payload.name,
      notification: payload.notification_type ?? payload.notificationType ?? payload.type,
      prompt: ['UserPromptSubmit', 'beforeSubmitPrompt', 'BeforeAgent'].includes(eventName) ? text(payload.prompt) : undefined };
    if (external) { event.workspaceRoot = workspaceRootOf(payload); event.external = true; }
    const debug = process.env.QUE_HARNESS_DEBUG === '1';
    // Keep only field metadata, never prompt/reply text, to diagnose missing previews.
    if (debug && kind === 'codex' && directory && eventName === 'Stop') {
      try {
        const value = payload.last_assistant_message;
        const diagnostic = { at, event: eventName, sessionId: event.sessionId,
          replyFieldPresent: Object.hasOwn(payload, 'last_assistant_message'),
          replyFieldType: value === null ? 'null' : typeof value,
          replyLength: typeof value === 'string' ? value.length : 0,
          previewLength: event.replyPreview?.length ?? 0 };
        const target = path.join(directory, 'last-stop-diagnostic.json');
        const temporary = `${target}.${randomUUID()}.tmp`;
        fs.writeFileSync(temporary, JSON.stringify(diagnostic), { mode: 0o600 });
        fs.renameSync(temporary, target);
      } catch {}
    }
    const channel = token;
    const delivered = { osc: false, file: false, oscError: undefined, fileError: undefined };
    if (channel) {
      try {
        const signal = Buffer.from(JSON.stringify({ token: channel, signal: event })).toString('base64');
        fs.writeFileSync(process.env.QUE_HARNESS_TTY || '/dev/tty', `\x1b]777;que;${signal}\x07`);
        delivered.osc = true;
      } catch (error) {
        delivered.oscError = error instanceof Error ? error.message : String(error);
      }
    }
    if (directory) {
      try {
        // Card sinks are pre-created by the app; the external sink has no owner yet.
        fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
        const target = path.join(directory, `${at}-${randomUUID()}.json`);
        fs.writeFileSync(`${target}.tmp`, JSON.stringify(event), { mode: 0o600 });
        fs.renameSync(`${target}.tmp`, target);
        delivered.file = true;
      } catch (error) {
        delivered.fileError = error instanceof Error ? error.message : String(error);
      }
      if (debug) {
        try {
          fs.appendFileSync(path.join(directory, 'hook-trace.jsonl'), `${JSON.stringify({ at, event: eventName, sessionId: event.sessionId, ...delivered })}\n`);
        } catch { /* Trace must not block the CLI. */ }
      }
      if (external) pruneExternal(directory);
    }
  } catch { /* Observation cannot block the CLI or emit model-visible text. */ }
  finally {
    finish();
  }
}
