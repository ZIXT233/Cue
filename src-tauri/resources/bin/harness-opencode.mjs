import fs from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';

// Lifecycle boundaries follow Orca's OpenCode status plugin; see docs/harness/opencode.
export const TopCardState = async ({ client }) => {
  const sessions = new Map();
  let owner = process.env.TOPCARD_HARNESS_SESSION_ID;
  let sequence = Promise.resolve();
  let lastSignalAt = 0;
  const enqueue = work => (sequence = sequence.then(work).catch(() => {}));

  const emit = (event, info, extra = {}) => {
    try {
      const signal = {
        kind: 'opencode',
        at: (lastSignalAt = Math.max(Date.now(), lastSignalAt + 1)),
        event,
        sessionId: info.id,
        title: typeof info.title === 'string' ? info.title.slice(0, 160) : undefined,
        ...extra,
      };
      const token = process.env.TOPCARD_HARNESS_CHANNEL;
      if (token) {
        fs.writeFileSync(
          process.env.TOPCARD_HARNESS_TTY || '/dev/tty',
          `\x1b]777;topcard;${Buffer.from(JSON.stringify({ token, signal })).toString('base64')}\x07`,
        );
      } else if (process.env.TOPCARD_HARNESS_SIGNAL_DIR) {
        const file = path.join(process.env.TOPCARD_HARNESS_SIGNAL_DIR, `${signal.at}-${randomUUID()}.json`);
        fs.writeFileSync(`${file}.tmp`, JSON.stringify(signal), { mode: 0o600 });
        fs.renameSync(`${file}.tmp`, file);
      }
    } catch { /* Observation cannot block the TUI. */ }
  };

  async function infoFor(id) {
    if (sessions.has(id)) return sessions.get(id);
    const signal = typeof AbortSignal?.timeout === 'function' ? AbortSignal.timeout(1500) : undefined;
    // Orca supports both OpenCode SDK generations; retain that compatibility boundary.
    const calls = client?.session?.get?.length >= 2
      ? [[{ sessionID: id }, { signal }], [{ path: { id }, signal }]]
      : [[{ path: { id }, signal }], [{ sessionID: id }]];
    for (const args of calls) {
      if (signal?.aborted) break;
      try {
        const response = await client.session.get(...args);
        const data = response?.data ?? response;
        if (data?.id === id) {
          if (sessions.size >= 256) sessions.delete(sessions.keys().next().value);
          sessions.set(id, data);
          return data;
        }
      } catch { /* Fall through; status events must not depend on get(). */ }
    }
  }

  function isChild(info) {
    return !!(info?.parentID || info?.parentId || info?.parent_id);
  }

  async function resolve(id, hint) {
    if (hint?.id === id) {
      sessions.set(id, hint);
      return hint;
    }
    return (await infoFor(id)) || { id };
  }

  function previewText(value) {
    if (typeof value !== 'string') return undefined;
    const text = value.replace(/[\x00-\x1f\x7f]/g, ' ').replace(/\s+/g, ' ').trim();
    return text ? text.slice(0, 160) : undefined;
  }

  async function replyPreview(sessionId) {
    if (!client?.session?.messages) return undefined;
    const signal = typeof AbortSignal?.timeout === 'function' ? AbortSignal.timeout(1500) : undefined;
    const attempts = [
      [{ path: { id: sessionId }, signal }],
      [{ sessionID: sessionId }, { signal }],
    ];
    for (const args of attempts) {
      try {
        const response = await client.session.messages(...args);
        const rows = response?.data ?? response;
        if (!Array.isArray(rows)) continue;
        for (let index = rows.length - 1; index >= 0; index--) {
          const row = rows[index];
          const info = row?.info ?? row;
          const role = info?.role ?? row?.role;
          if (role !== 'assistant') continue;
          const parts = row?.parts ?? info?.parts ?? info?.content;
          if (typeof parts === 'string') return previewText(parts);
          if (!Array.isArray(parts)) continue;
          const text = parts
            .map(part => (typeof part === 'string' ? part : part?.type === 'text' ? part.text : ''))
            .filter(Boolean)
            .join(' ');
          const preview = previewText(text);
          if (preview) return preview;
        }
      } catch { /* Preview is optional for completion notifications. */ }
    }
  }

  return {
    'chat.message': (input) => enqueue(async () => {
      const sessionID = input?.sessionID || input?.sessionId;
      if (!sessionID) return;
      const info = await resolve(sessionID);
      if (isChild(info)) return;
      owner = info.id;
      emit('UserPromptSubmit', info);
    }),
    event: ({ event }) => {
      // Serialize async identity lookups so older idle events cannot pass newer busy events.
      if (![
        'session.created', 'session.updated', 'session.status', 'session.idle',
        'permission.asked', 'question.asked', 'permission.replied', 'question.replied', 'question.rejected',
      ].includes(event.type)) return;
      return enqueue(async () => {
        const properties = event.properties || {};
        if (properties.info?.id && event.type.startsWith('session.')) sessions.set(properties.info.id, properties.info);
        const id = properties.sessionID || properties.sessionId || properties.info?.id;
        if (!id) return;
        const info = await resolve(id, properties.info);
        if (isChild(info)) return;
        const status = properties.status?.type ?? (typeof properties.status === 'string' ? properties.status : undefined);
        if (!owner || status === 'busy' || status === 'retry') owner = id;
        if (id !== owner) return;
        if (event.type === 'session.created') emit('SessionStart', info);
        else if (event.type === 'session.updated') emit('SessionInfo', info);
        else if (event.type === 'permission.asked' || event.type === 'question.asked') emit('PermissionRequest', info);
        else if (['permission.replied', 'question.replied', 'question.rejected'].includes(event.type)) emit('UserPromptSubmit', info);
        else if (status === 'busy' || status === 'retry') emit('UserPromptSubmit', info);
        else if (status === 'idle' || event.type === 'session.idle') {
          emit('Stop', info, { replyPreview: await replyPreview(id) });
        }
      });
    },
  };
};

export default TopCardState;
