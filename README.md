# Cue

**English** | [简体中文](README.zh-CN.md)

**Queue of Agent Cues**

Cue schedules your attention with a unified queue of waiting Agent sessions, so you can just handle the one at the front instead of chasing notification badges.

Cue supports mainstream CLI harnesses through terminal cards, so you can handle multiple harness workflows in one place.

## Stack

- Frontend: Vite + React
- Desktop: Tauri 2
- Backend: Rust (queue, PTY, harness hooks/OSC, workspaces, SSH)

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- npm (ships with Node.js)
- [Rust](https://www.rust-lang.org/tools/install) stable (`rustup`)
- Platform toolchain for Tauri 2: see [Prerequisites](https://v2.tauri.app/start/prerequisites/)
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio C++ workload + WebView2
  - Linux: `webkit2gtk` and the rest of the Tauri system packages

```bash
node -v
npm -v
rustc -V
cargo -V
npx tauri --version
```

## Setup

```bash
git clone https://github.com/ZIXT233/Cue.git
cd Cue
npm install
```

Rust crates download on the first `tauri` / `cargo` run (`src-tauri/`).

## Develop

Desktop app (starts Vite on port `1420`, then the Tauri window):

```bash
npm run tauri dev
```

Frontend only (browser, no Rust / PTY / desktop APIs):

```bash
npm run dev
```

Typecheck:

```bash
npx tsc --noEmit
```

Diagnostics:

```bash
npx tauri info
```

## Build

### Frontend

```bash
npm run build
```

Runs `tsc && vite build`. Output: `dist/`.

Preview the production frontend:

```bash
npm run preview
```

### Desktop

Release build for the current platform (also runs `npm run build` first):

```bash
npm run tauri build
```

Debug desktop build (faster, larger, includes debug symbols):

```bash
npm run tauri -- build --debug
```

### Bundles

Default `targets` in `src-tauri/tauri.conf.json` is `all` for the current OS.

```bash
# macOS
npm run tauri -- build --bundles app
npm run tauri -- build --bundles dmg

# Windows
npm run tauri -- build --bundles nsis
npm run tauri -- build --bundles msi

# Linux
npm run tauri -- build --bundles deb
npm run tauri -- build --bundles rpm
npm run tauri -- build --bundles appimage
```

macOS universal binary (Apple Silicon + Intel):

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri -- build --target universal-apple-darwin
```

### Rust crate only

Does not produce an installable app bundle.

```bash
cd src-tauri
cargo check
cargo build
cargo build --release
```

### Icons

Regenerate platform icons from the mark:

```bash
npx tauri icon src-tauri/icons/cue-mark.svg
```

Packaged Dock / Start Menu icons need a fresh desktop build (or reinstall). `tauri dev` may keep a cached icon.

## Artifacts

| Kind | Path |
| --- | --- |
| Frontend | `dist/` |
| Rust binary | `src-tauri/target/release/cue` |
| macOS app | `src-tauri/target/release/bundle/macos/Cue.app` |
| macOS dmg | `src-tauri/target/release/bundle/dmg/` |
| Windows | `src-tauri/target/release/bundle/nsis/`, `msi/` |
| Linux | `src-tauri/target/release/bundle/deb/`, `rpm/`, `appimage/` |
| Debug bundles | `src-tauri/target/debug/bundle/` |

`node_modules/`, `dist/`, and `src-tauri/target/` are gitignored.

## Notes

App data lives in `~/.cue`.

Harness hook environment variables stay `TOPCARD_HARNESS_*` so CLI communication matches the original protocol.
