# Que

**English** | [简体中文](README.zh-CN.md)

**让 Agent 排队找你**

Que 通过统一的待处理 Agent 会话队列调度你的注意力，让你只需处理队首，而不用追着通知小红点跑。

Que 通过终端卡片支持主流 CLI Harness，让你可以在同一个地方处理多个 Harness 工作流。

## 工作流

### 待回复卡片队列

- 每个会话一张卡片，状态实时更新。正在干活的卡片不占队列——队里排的就是在等回复的。
- 排序两种：评分（等待时长 + 卡片权重 + 标签加分）或先进先出；「紧急呼叫」标签置顶。「稍后提醒」的卡片到点自动回队。
- 处理完一张，下一张顶上；干完的归档，跑着的可以拆成独立窗口盯着。
- 会话需要你时弹桌面通知，点一下直接跳到那张卡片。

### 多CLI Harness响应支持

Claude Code、CodeBuddy、Codex、Cursor、Antigravity、Gemini、Grok、OpenCode、Pi / OMP，外加一张万能的 Shell 卡片。

- 从卡片启动会话时自动完成接入，不需要你手动配 hook；你手写的配置条目永远不会被覆盖。
- 卡片上显示真实的会话标题和提问内容，重开一张卡会自动接回原来的对话。
- 各家 Harness 的实测状态（消息发送 / 普通回复 / ask / perm / resume 同步 / 标题来源）：

| Harness | 消息发送 | 普通回复 | ask | perm | resume 同步 | 标题 |
| --- | --- | --- | --- | --- | --- | --- |
| Codex | 正常 | 正常 | 正常 | 正常 | 正常 | thread_name |
| Claude Code | 正常 | 正常 | 正常 | 正常 | 正常 | custom-title 或 会话第一条prompt |
| CodeBuddy | 正常 | 正常 | 正常 | 正常 | 正常 | custom-title 或 ai-title |
| Cursor | 正常 | 正常 | 正常 | 正常 | 通过 | meta.title |
| Pi | 正常 | 正常 | 正常 | 正常 | 正常 | session_info.name |
| OMP | 正常 | 正常 | 正常 | 正常 | 正常 | title |
| Grok | 正常 | 正常 | 正常 | 正常 | 正常 | generated_title |
| Antigravity | 正常 | 正常 | 正常 | 正常 | 正常 | hook最后一次prompt |
| OpenCode | 正常 | 正常 | 正常 | 正常 | 正常 | OSC info.title |
| Shell | | | | | | 工作区名 |

### 外部Agent会话捕捉

IDE 聊天窗口、裸终端里跑的同一批 Harness 也会向你要注意力。装好用户级 hook 后，Que 会把这些询问一并抓进来：以通知卡的形式出现在同一个牌堆里，带着项目名、问题和到目前为止的对话，会话回到工作状态时自动消失；每一家的外部抓取都可以在设置里单独开关。

### 远程工作区支持

- SSH 主机保存一次，选中上面的一个目录，之后卡片、状态、通知和本地完全一致。
- 远程机器上需要装好要用的 CLI 和 Node.js 22+。
- 启动卡片前会自动检查 CLI 和 Node 是否就位，缺了直接告诉你缺什么，不会开出一张死终端让你自己猜。

## 开发

环境要求：Node.js 22+、Rust 工具链，以及你所在平台的 [Tauri 2 依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run tauri dev     # 跑桌面端
npm run tauri build   # 打当前平台的安装包
```

应用数据在 `~/.que`。协议细节与调试指南：[docs/harness/hook-api.zh-CN.md](docs/harness/hook-api.zh-CN.md)。
