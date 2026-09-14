# Cue

[English](README.md) | **简体中文**

**让 Agent 排队找你**

Cue 通过统一的待处理 Agent 会话队列调度你的注意力，让你只需处理队首，而不用追着通知小红点跑。

Cue 通过终端卡片支持主流 CLI Harness，让你可以在同一个地方处理多个 Harness 工作流。

## 技术栈

- 前端：Vite + React
- 桌面：Tauri 2
- 后端：Rust（队列、PTY、Harness hooks/OSC、工作区、SSH）

## 环境要求

- [Node.js](https://nodejs.org/) 18+
- npm（随 Node.js 安装）
- [Rust](https://www.rust-lang.org/tools/install) stable（`rustup`）
- Tauri 2 各平台工具链：见 [Prerequisites](https://v2.tauri.app/start/prerequisites/)
  - macOS：Xcode Command Line Tools
  - Windows：Visual Studio C++ 工作负载 + WebView2
  - Linux：`webkit2gtk` 以及其余 Tauri 系统依赖

```bash
node -v
npm -v
rustc -V
cargo -V
npx tauri --version
```

## 安装

```bash
git clone https://github.com/ZIXT233/Cue.git
cd Cue
npm install
```

首次跑 `tauri` / `cargo` 时会下载 Rust 依赖（`src-tauri/`）。

## 开发

桌面端（先起 Vite，端口 `1420`，再开 Tauri 窗口）：

```bash
npm run tauri dev
```

只跑前端（浏览器，没有 Rust / PTY / 桌面 API）：

```bash
npm run dev
```

类型检查：

```bash
npx tsc --noEmit
```

环境诊断：

```bash
npx tauri info
```

## 构建

### 前端

```bash
npm run build
```

实际执行 `tsc && vite build`。产物在 `dist/`。

预览生产前端：

```bash
npm run preview
```

### 桌面端

当前平台的 Release 包（会先跑 `npm run build`）：

```bash
npm run tauri build
```

Debug 桌面包（更快、体积更大、带调试符号）：

```bash
npm run tauri -- build --debug
```

### 安装包

`src-tauri/tauri.conf.json` 里 `targets` 默认是当前系统的 `all`。

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

macOS 通用二进制（Apple Silicon + Intel）：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri -- build --target universal-apple-darwin
```

### 只编 Rust

不会打出可安装的应用包。

```bash
cd src-tauri
cargo check
cargo build
cargo build --release
```

### 图标

从源标重新生成各平台图标：

```bash
npx tauri icon src-tauri/icons/cue-mark.svg
```

安装包里的 Dock / 开始菜单图标需要重新打桌面包（或重装）。`tauri dev` 可能继续用缓存图标。

## 产物位置

| 类型 | 路径 |
| --- | --- |
| 前端 | `dist/` |
| Rust 二进制 | `src-tauri/target/release/cue` |
| macOS app | `src-tauri/target/release/bundle/macos/Cue.app` |
| macOS dmg | `src-tauri/target/release/bundle/dmg/` |
| Windows | `src-tauri/target/release/bundle/nsis/`、`msi/` |
| Linux | `src-tauri/target/release/bundle/deb/`、`rpm/`、`appimage/` |
| Debug 包 | `src-tauri/target/debug/bundle/` |

`node_modules/`、`dist/`、`src-tauri/target/` 已加入 gitignore。

## 说明

应用数据在 `~/.cue`。

Harness hook 环境变量仍是 `TOPCARD_HARNESS_*`，以便和原有 CLI 协议对齐。
