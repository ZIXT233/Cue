import { isHarnessLaunchAction, launchCardHarness } from "@/lib/harness/card-launch";
import { codexSessionTitle } from "@/lib/harness/codex-title";
import { launchHarness, harnessSnapshot } from "@/lib/harness/runtime";
import { stopTerminal, killTerminal } from "@/lib/terminal-manager";
import { validatePromptSources } from "@/lib/prompt-sources";
import { isReservedUrgentName } from "@/lib/urgent-call";
import { numericWeight, validateTags, effectiveTurnTags } from "@/lib/turn-priority";
import { NextResponse } from "next/server";
import { randomUUID } from "node:crypto";
import { stat, mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { homedir } from "node:os";
import { withCardQueue } from "@/lib/card-queue-store";
import { ensureQueueLiveWatch } from "@/lib/card-queue-live";
import { reconcileQueue, archiveCard, deferCard, moveCard, releaseCard, TAB_LEASE_MS, type CardQueue } from "@/lib/card-queue";
import { getRpcSession, getRpcSessionInfos, getRunningRpcSessionIds } from "@/lib/rpc-manager";
import { listAllSessions, mergeSessionLists } from "@/lib/session-reader";
import { allowFileRoot } from "@/lib/file-access";
import { sshExec, shellQuote } from "@/lib/ssh-workspace";
import { SessionManager } from "@earendil-works/pi-coding-agent";
import { inheritForkTurn } from "@/lib/fork-turn";

export const dynamic = "force-dynamic";

async function sync(state: CardQueue) {
  state.workspaces ??= [];
  if (!state.sortMode) { state.sortMode = "score"; state.insertionPosition = "bottom"; }
  state.insertionPosition ??= "bottom";
  delete (state as CardQueue & { sortOrder?: unknown }).sortOrder;
  // Migrate the first prototype's directory-per-card model once.
  for (const card of state.cards) {
    if (card.workspaceId) continue;
    let workspace = state.workspaces.find((item) => item.runtimeCwd === card.cwd);
    if (!workspace) {
      workspace = { id: randomUUID(), name: card.cwd.split("/").filter(Boolean).pop() || card.cwd, cwd: card.cwd, runtimeCwd: card.cwd, kind: "local" };
      state.workspaces.push(workspace);
    }
    card.workspaceId = workspace.id;
  }
  // Restore roots from saved workspaces before rendering existing draft cards.
  for (const workspace of state.workspaces) allowFileRoot(workspace.runtimeCwd);
  const sessions = mergeSessionLists(await listAllSessions(), getRpcSessionInfos());
  const running = new Set(getRunningRpcSessionIds());
  const attention = new Set(state.cards.flatMap((card) => card.session && getRpcSession(card.session.id)?.needsHumanInput() ? [card.session.id] : []));
  for (const card of state.cards) {
    if (!card.harness) continue;
    if (card.archivedAt === undefined || (!card.harness.providerSessionId && card.harness.unpersistedSession === undefined)) card.harness = await harnessSnapshot(card.harness);
    else if (card.harness.kind === "codex" && card.harness.providerSessionId && !card.harness.remote) card.harness.title = await codexSessionTitle(card.harness.providerSessionId) ?? card.harness.title;
  }
  Object.assign(state, reconcileQueue(state, running, attention));
  for (const card of state.cards) {
    const session = sessions.find((item) => item.id === card.session?.id);
    if (session) card.session = session;
    if (card.phase === "attention" && card.archivedAt === undefined && card.session)
      getRpcSession(card.session.id)?.evaluateWaitingTurn?.();
  }
  return sessions;
}

function selectWorkspaceForDraft(state: CardQueue, workspace: NonNullable<CardQueue["workspaces"]>[number]) {
  // Saved workspaces survive restarts; the in-memory file roots do not.
  // Authorize the selected runtime directory before the draft loads models.
  allowFileRoot(workspace.runtimeCwd);
  const draft = state.cards.find((item) => !item.session && !item.harness);
  if (draft) {
    draft.cwd = workspace.runtimeCwd;
    draft.workspaceId = workspace.id;
    draft.priorityWeight = workspace.defaultConversationWeight ?? 0;
    delete draft.detached;
    state.order = state.order.filter(id => id !== draft.id);
    return;
  }
  const id = randomUUID();
  state.cards.push({ priorityWeight: workspace.defaultConversationWeight ?? 0, id, cwd: workspace.runtimeCwd, workspaceId: workspace.id, session: null, phase: "draft", createdAt: Date.now() });
}

export async function GET() {
  const started = Date.now();
  try {
    await ensureQueueLiveWatch();
    const state = await withCardQueue(async (state) => { await sync(state); return state; }, { silent: true });
    console.log(`[card-queue] ${Date.now() - started}ms cards=${state.cards.length}`);
    return NextResponse.json({ ...state, defaultCwd: process.cwd() }, { headers: { "Cache-Control": "no-store" } });
  } catch (error) {
    console.log(`[card-queue] ${Date.now() - started}ms error=${String(error)}`);
    return NextResponse.json({ error: String(error) }, { status: 500 });
  }
}

export async function POST(req: Request) {
  try {
    const body = await req.json();
    if (isHarnessLaunchAction(body.action)) {
      if (typeof body.id !== "string") throw new Error("卡片 ID 无效");
      const state = await launchCardHarness(body.id, body.action, body.kind, { sync, launch: launchHarness, kill: killTerminal });
      return NextResponse.json(state, { headers: { "Cache-Control": "no-store" } });
    }
    const state = await withCardQueue(async (state) => {
      const sessions = await sync(state);
      const card = state.cards.find((item) => item.id === body.id);
      switch (body.action) {
        case "shell_background": {
          if (!card?.harness || card.harness.kind !== "shell" || card.harness.shellCommandNotifications === false || !card.harness.shellCommandRunning || Date.now() - (card.harness.shellCommandStartedAt ?? Date.now()) < 300) throw new Error("命令已结束或尚未运行 300ms");
          card.harness.shellNotify = true;
          break;
        }
        case "harness_close": {
          if (!card?.harness) throw new Error("CLI 卡片不存在");
          stopTerminal(card.harness.terminalId);
          card.phase = "attention";
          card.harness.state = "exited";
          delete card.detached;
          archiveCard(state, card.id);
          break;
        }
        case "sort_mode":
          if (!["fifo", "score"].includes(body.mode)) throw new Error("无效排序模式");
          state.sortMode = body.mode;
          state.insertionPosition = "bottom";
          break;
        case "turn_tags_enabled":
          if (typeof body.enabled !== "boolean") throw new Error("无效开关值");
          state.turnTagsEnabled = body.enabled;
          break;
        case "turn_tags":
          if (!Array.isArray(body.tags) || body.tags.some((tag: { name?: unknown }) => typeof tag?.name === "string" && isReservedUrgentName(tag.name))) throw new Error("Urgent Call 是不可自定义的内置标签");
          state.turnTagDefinitions = effectiveTurnTags(validateTags(body.tags));
          break;
        case "workspace_weight": {
          const workspace = state.workspaces?.find((item) => item.id === body.workspaceId);
          if (!workspace) throw new Error("工作区不存在");
          workspace.defaultConversationWeight = numericWeight(body.weight);
          break;
        }
        case "workspace_update": {
          const workspace = state.workspaces?.find((item) => item.id === body.workspaceId);
          if (!workspace) throw new Error("工作区不存在");
          if (typeof body.name !== "string" || !body.name.trim()) throw new Error("请输入工作区名称");
          workspace.name = body.name.trim();
          workspace.defaultConversationWeight = numericWeight(body.defaultConversationWeight ?? 0);
          break;
        }
        case "priority_weight":
          if (!card) throw new Error("卡片不存在");
          card.priorityWeight = numericWeight(body.weight);
          break;
        case "reset_wait":
          if (!card || card.phase !== "attention") throw new Error("卡片未在等待处理");
          card.waitingSince = Date.now();
          break;
        case "insertion_position":
          if (body.position !== "top" && body.position !== "bottom") throw new Error("请选择顶部插入或底部插入");
          state.insertionPosition = body.position;
          break;
        case "workspace_create": {
          if (typeof body.name !== "string" || !body.name.trim()) throw new Error("请输入工作区名称");
          if (typeof body.cwd !== "string" || !body.cwd.trim()) throw new Error("请输入工作区目录");
          if (!["local", "ssh"].includes(body.kind)) throw new Error("请选择工作区位置");
          let cwd: string;
          if (body.kind === "local") {
            cwd = resolve(body.cwd.trim().replace(/^~(?=\/|$)/, homedir()));
            if (!(await stat(cwd)).isDirectory()) throw new Error("工作目录不存在");
            allowFileRoot(cwd);
          } else {
            if (typeof body.sshHost !== "string" || !/^[a-zA-Z0-9][a-zA-Z0-9._@:-]*$/.test(body.sshHost)) throw new Error("请输入 SSH 主机别名或 user@host");
            if (!body.cwd.startsWith("/")) throw new Error("SSH 工作目录请使用绝对路径");
            cwd = (await sshExec(body.sshHost, `cd ${shellQuote(body.cwd)} && pwd -P`)).toString().trim();
          }
          let workspace = state.workspaces?.find((item) => item.kind === body.kind && item.cwd === cwd && item.sshHost === body.sshHost);
          if (!workspace) {
            const id = randomUUID();
            const runtimeCwd = body.kind === "local" ? cwd : resolve(process.env.TOPCARD_DATA_DIR || resolve(process.cwd(), ".topcard"), "ssh", id);
            if (body.kind === "ssh") {
              await mkdir(runtimeCwd, { recursive: true });
              await writeFile(resolve(runtimeCwd, "remote-workspace.json"), JSON.stringify({ sshHost: body.sshHost, cwd }), { mode: 0o600 });
            }
            workspace = { defaultConversationWeight: numericWeight(body.defaultConversationWeight ?? 0), id, name: body.name.trim(), kind: body.kind, cwd, runtimeCwd, ...(body.kind === "ssh" ? { sshHost: body.sshHost } : {}) };
            state.workspaces!.push(workspace);
          } else {
            workspace.name = body.name.trim();
            workspace.defaultConversationWeight = numericWeight(body.defaultConversationWeight ?? 0);
          }
          if (body.createCard === true) selectWorkspaceForDraft(state, workspace);
          break;
        }
        case "workspace_remove": {
          const workspace = state.workspaces?.find((item) => item.id === body.workspaceId);
          if (!workspace) throw new Error("工作区不存在");
          const removedCardIds = new Set(state.cards.filter((item) => item.workspaceId === workspace.id).map((item) => item.id));
          for (const item of state.cards) if (removedCardIds.has(item.id) && item.harness) killTerminal(item.harness.terminalId);
          state.cards = state.cards.filter((item) => !removedCardIds.has(item.id));
          state.order = state.order.filter((id) => !removedCardIds.has(id));
          state.workspaces = state.workspaces!.filter((item) => item.id !== workspace.id);
          break;
        }
        case "prompt_sources": {
          if (!card || card.session) throw new Error("会话开始后提示词只读，请应用到新会话");
          card.promptSources = validatePromptSources(body.config);
          break;
        }
        case "create": {
          const workspace = state.workspaces?.find((item) => item.id === body.workspaceId);
          if (!workspace) throw new Error("请选择一个工作区");
          const config = body.promptSources === undefined ? undefined : validatePromptSources(body.promptSources);
          selectWorkspaceForDraft(state, workspace);
          if (config) {
            const draft = state.cards.find(item => !item.session && !item.harness && item.workspaceId === workspace.id);
            if (draft) draft.promptSources = config;
          }
          break;
        }
        case "adopt": {
          if (state.cards.some(item => item.harness?.kind === "pi" && item.harness.providerSessionId === body.sessionId)) throw new Error("此会话已关联 Pi CLI 卡片，请从该卡片继续");
          const session = sessions.find((item) => item.id === body.sessionId);
          if (!session) throw new Error("找不到这个 Pi 会话");
          const existing = state.cards.find((item) => item.session?.id === session.id);
          if (existing) {
            if (body.fork === true && !existing.turnKey) inheritForkTurn(existing, SessionManager.open(session.path).buildContextEntries());
            moveCard(state, existing.id, "front");
          }
          else {
            const id = randomUUID();
            const created: CardQueue["cards"][number] = { id, cwd: session.cwd, session, phase: "attention", createdAt: Date.now() };
            if (body.fork === true) inheritForkTurn(created, SessionManager.open(session.path).buildContextEntries());
            state.cards.push(created);
            state.order.unshift(id);
          }
          break;
        }
        case "attach": {
          if (!card) throw new Error("卡片已不存在");
          const session = sessions.find((item) => item.id === body.sessionId);
          if (!session) throw new Error("Pi 会话尚未就绪，请重试");
          if (card.session && card.session.id !== session.id) throw new Error("卡片已关联另一会话");
          // A fork or a second browser must not duplicate the same session card.
          const duplicate = state.cards.find((item) => item.id !== card.id && item.session?.id === session.id);
          if (duplicate) throw new Error("会话已在队列中");
          card.session = session;
          break;
        }
        case "defer":
          deferCard(state, body.id);
          break;
        case "front": case "back":
          moveCard(state, body.id, body.action);
          break;
        case "archive":
          if (card?.session && getRpcSession(card.session.id)?.isRunning()) throw new Error("请先处理等待中的交互，或停止正在运行的会话");
          archiveCard(state, body.id);
          if (card?.harness) { stopTerminal(card.harness.terminalId); card.harness.state = "exited"; }
          break;
        case "restore":
          if (!card) throw new Error("卡片已不存在");
          delete card.archivedAt;
          moveCard(state, body.id, "front");
          break;
        case "remove":
          if (card?.phase === "working" || (card?.session && getRpcSession(card.session.id)?.isRunning())) throw new Error("请先处理等待中的交互，或停止正在运行的会话");
          if (card?.detached) throw new Error("请先收回独立标签页");
          if (card?.harness) killTerminal(card.harness.terminalId);
          state.cards = state.cards.filter((item) => item.id !== body.id);
          state.order = state.order.filter((id) => id !== body.id);
          break;
        case "claim":
          if (!card || typeof body.owner !== "string" || !body.owner) throw new Error("无效的标签页");
          if (card.detached && card.detached.owner !== body.owner) throw new Error("该卡片已经在另一个标签页打开");
          card.detached = { owner: body.owner, expiresAt: Date.now() + TAB_LEASE_MS };
          break;
        case "release":
          if (card) releaseCard(card, body.owner);
          break;
        default: throw new Error("未知队列操作");
      }
      await sync(state);
      return state;
    });
    return NextResponse.json(state, { headers: { "Cache-Control": "no-store" } });
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : String(error) }, { status: 400 });
  }
}
