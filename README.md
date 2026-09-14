# Cue

**Queue of Agent Cues**

Cue schedules your attention with a unified queue of waiting Agent sessions, so you can just handle the one at the front instead of chasing notification badges.

Cue supports mainstream CLI harnesses through terminal cards, so you can handle multiple harness workflows in one place.

---

# Cue

**让 Agent 排队找你**

Cue 通过统一的待处理 Agent 会话队列调度你的注意力，让你只需处理队首，而不用追着通知小红点跑。

Cue 通过终端卡片支持主流 CLI Harness，让你可以在同一个地方处理多个 Harness 工作流。

## Stack

- Frontend: Vite + React (card-queue UI ported from TopCard)
- Desktop: Tauri 2
- Backend: Rust (queue, PTY, harness hooks/OSC, workspaces, SSH)

## Develop

```bash
npm install
npm run tauri dev
```

Data lives in `~/.cue`. Harness hook environment variables stay `TOPCARD_HARNESS_*` so CLI communication matches the original protocol.
