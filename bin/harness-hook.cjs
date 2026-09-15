/* eslint-disable @typescript-eslint/no-require-imports -- Standalone passive CLI hook. */
// Shared ingress for built-in CLI adapters. Public contract: docs/harness/hook-api.md
const fs = require('node:fs');
const path = require('node:path');
const { randomUUID } = require('node:crypto');
const explicitEvent = process.argv[2];
const cursorEvents = new Set(['sessionStart', 'beforeSubmitPrompt', 'preToolUse', 'postToolUse', 'postToolUseFailure', 'beforeShellExecution', 'beforeMCPExecution', 'afterAgentResponse', 'stop', 'sessionEnd']);
function cursorReply(event) {
  if (event === 'beforeSubmitPrompt') return { continue: true };
  if (event === 'beforeShellExecution' || event === 'beforeMCPExecution') return { permission: 'allow' };
  return {};
}
const kind = process.env.CUE_HARNESS_KIND || (cursorEvents.has(explicitEvent) ? 'cursor' : undefined);
const token = process.env.CUE_HARNESS_CHANNEL;
const envDirectory = process.env.CUE_HARNESS_SIGNAL_DIR;
const activePath = path.join(__dirname, 'active.json');
// Cursor observe hooks ignore stdout, but answering before stdin is fully read
// lets the worker tear the process down before replyPreview is written. Reply
// after the signal (beforeSubmitPrompt still returns continue:true).
if (kind === 'gemini' || kind === 'grok') process.stdout.write('{}\n');
if (kind === 'cursor' && !envDirectory && !token) {
  // Global ~/.cursor/hooks.json also fires for IDE / other agents. Only Cue-launched
  // processes inherit SIGNAL_DIR or CHANNEL; ignore the rest after the required reply.
  process.stdout.write(JSON.stringify(cursorReply(explicitEvent)) + '\n');
  process.exit(0);
}
if (kind !== 'cursor' && !envDirectory && !token && !legacyActiveDirectory()) {
  process.exit(0);
}
const at = Date.now();
let input = '', oversized = false, done = false, finished = false;
const timer = setTimeout(() => { consume(); finish(); }, 8000);
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
    if (kind === 'antigravity') {
      if (eventName === 'Stop' && (payload.fullyIdle === false || payload.fully_idle === false)) eventName = 'PreInvocation';
      if (eventName === 'PreToolUse' && ['ask_question', 'ask_permission'].includes(payload.toolCall?.name)) eventName = 'PermissionRequest';
    }
    const text = value => typeof value === 'string' ? value.replace(/[\x00-\x1f\x7f]/g, ' ').trim().slice(0, 160) : undefined;
    const completion = eventName === 'afterAgentResponse' || ['Stop', 'stop', 'AfterAgent'].includes(eventName);
    const event = { kind, at, event: eventName, replyPreview: completion ? text(replyText(payload)) : undefined, sessionId: payload.conversationId || payload.conversation_id || payload.session_id || payload.sessionId,
      agentId: payload.agent_id || payload.agentId, tool: payload.toolCall?.name ?? payload.tool_name ?? payload.toolName ?? payload.name,
      notification: payload.notification_type ?? payload.notificationType ?? payload.type,
      prompt: ['UserPromptSubmit', 'beforeSubmitPrompt', 'BeforeAgent'].includes(eventName) ? text(payload.prompt) : undefined };
    const directory = envDirectory || (kind === 'cursor' ? undefined : legacyActiveDirectory());
    const debug = process.env.CUE_HARNESS_DEBUG === '1';
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
        fs.writeFileSync(process.env.CUE_HARNESS_TTY || '/dev/tty', `\x1b]777;cue;${signal}\x07`);
        delivered.osc = true;
      } catch (error) {
        delivered.oscError = error instanceof Error ? error.message : String(error);
      }
    }
    if (directory) {
      try {
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
    }
  } catch { /* Observation cannot block the CLI or emit model-visible text. */ }
  finally {
    finish();
  }
}
