import fs from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';

// Loaded only by a TopCard-owned Pi CLI. No tools, prompts or permission changes.
export default function topcardState(pi) {
  function emit(event, ctx, prompt) {
    try {
      const text = value => typeof value === 'string' ? value.replace(/[\x00-\x1f\x7f]/g, ' ').trim().slice(0, 160) : undefined;
      const messages = event === 'Stop' ? ctx.sessionManager.buildSessionContext().messages : [];
      const last = [...messages].reverse().find(message => message.role === 'assistant');
      const replyPreview = last ? text(typeof last.content === 'string' ? last.content : last.content.filter(block => block.type === 'text').map(block => block.text).join(' ')) : undefined;
      const signal = { replyPreview, at: Date.now(), event, sessionId: ctx.sessionManager.getSessionId(), title: text(ctx.sessionManager.getSessionName()), prompt: text(prompt) };
      const token = process.env.TOPCARD_HARNESS_CHANNEL;
      if (token) {
        fs.writeFileSync('/dev/tty', `\x1b]777;topcard;${Buffer.from(JSON.stringify({ token, signal })).toString('base64')}\x07`);
      } else if (process.env.TOPCARD_HARNESS_SIGNAL_DIR) {
        const target = path.join(process.env.TOPCARD_HARNESS_SIGNAL_DIR, `${signal.at}-${randomUUID()}.json`);
        fs.writeFileSync(`${target}.tmp`, JSON.stringify(signal), { mode: 0o600 });
        fs.renameSync(`${target}.tmp`, target);
      }
    } catch { /* A status observer must not interrupt Pi. */ }
  }
  let working = false;
  pi.on('session_start', (_event, ctx) => emit('SessionStart', ctx));
  pi.on('session_info_changed', (_event, ctx) => emit('SessionInfo', ctx));
  pi.on('before_agent_start', (event, ctx) => { emit('UserPromptSubmit', ctx, event.prompt); });
  pi.on('agent_start', (_event, ctx) => { working = true; emit('UserPromptSubmit', ctx); });
  // agent_end can precede retries/compaction. Only settled means done.
  pi.on('agent_settled', (_event, ctx) => { working = false; emit('Stop', ctx); });
  pi.on('ui_prompt_start', (_event, ctx) => emit('PermissionRequest', ctx));
  pi.on('ui_prompt_end', (_event, ctx) => emit(working ? 'UserPromptSubmit' : 'Stop', ctx));
}
