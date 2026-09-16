# Que

**English** | [简体中文](README.zh-CN.md)

**Queue of Agent Cues**

Que schedules your attention with a unified queue of waiting Agent sessions, so you can just handle the one at the front instead of chasing notification badges.

Que supports mainstream CLI harnesses through terminal cards, so you can handle multiple harness workflows in one place.

## How it works

### Pending-reply card queue

- One card per session with live state. Working sessions are not in the queue — the queue is exactly the cards waiting for a reply.
- Two sort modes: score (wait minutes + card weight + tag bonuses) or FIFO; the Urgent Call tag sorts first. Remind-later parks a card and it re-enters on its own.
- Finish one, the next steps up. Archive finished work, pop a running session out into its own window.
- When a session needs you, a desktop notification takes you straight to its card.

### Multi-CLI harness response support

Claude Code, CodeBuddy, Codex, Cursor, Antigravity, Gemini, Grok, OpenCode, Pi / OMP — plus a plain shell card for everything else.

- Launching a card sets the harness up automatically; no manual hook configuration, and config entries you wrote yourself are never overwritten.
- Cards show real session titles and prompts, and restarting a card picks the original conversation back up.
- Current checked state per harness (message send / normal reply / ask / permission / resume sync / title):

| Harness | Message send | Normal reply | Ask | Permission | Resume sync | Title |
| --- | --- | --- | --- | --- | --- | --- |
| Codex | OK | OK | OK | OK | OK | thread_name → first session prompt → last hooked prompt → Codex · workspace |
| Claude Code | OK | OK | OK | OK | | custom-title → first session prompt → last hooked prompt → Claude Code · workspace |
| CodeBuddy | | | | | | custom-title → ai-title → topic → first session prompt → last hooked prompt → CodeBuddy · workspace |
| Cursor | OK | OK | OK | OK | pass | meta.title → first prompt-history entry → last hooked prompt → Cursor Agent · workspace |
| Pi | OK | OK | OK | OK | OK | session_info.name → first session prompt → last hooked prompt → Pi CLI · workspace |
| OMP | OK | OK | OK | OK | OK | title → session_info.name → first session prompt → last hooked prompt → Oh My Pi · workspace |
| Grok | OK | OK | OK | OK | OK | summary.generated_title → first session prompt → last hooked prompt → Grok Build · workspace |
| Antigravity | | | | | | last hooked prompt → Antigravity CLI · workspace |
| OpenCode | OK | OK | OK | OK | OK | OSC info.title (default "New session" filtered) → first hooked prompt → last hooked prompt → OpenCode · workspace |
| Shell | | | | | | workspace name |

### External agent session capture

IDE chats and plain terminals run the same harnesses. With the user-level hooks installed, Que captures those asks too and shows them as notice cards in the same deck — with the project, the question, and the conversation so far. They disappear when the session goes back to work, and each harness's capture can be toggled in settings.

### Remote workspace support

- Save an SSH host once and pick a directory on it — cards, live state and notifications work exactly like local.
- The remote machine needs the harness CLI and Node.js 22+ installed.
- Both are checked before a card opens, so a missing dependency is a clear error message instead of a dead terminal you have to diagnose yourself.

## Development

Prerequisites: Node.js 22+, a Rust toolchain, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run tauri dev     # run the desktop app
npm run tauri build   # package the current OS
```

App data lives in `~/.que`. The harness wire contract and debugging guide: [docs/harness/hook-api.md](docs/harness/hook-api.md).
