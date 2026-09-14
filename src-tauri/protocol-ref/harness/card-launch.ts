import { withCardQueue } from "../card-queue-store.ts";
import type { CardQueue, QueueWorkspace } from "../card-queue.ts";
import type { HarnessSession } from "./types.ts";

type LaunchAction = "harness_start" | "harness_reopen" | "harness_restart" | "harness_resume";
export function isHarnessLaunchAction(action: unknown): action is LaunchAction {
  return ["harness_start", "harness_reopen", "harness_restart", "harness_resume"].includes(String(action));
}
const globals = globalThis as typeof globalThis & { __topcardCardLaunches?: Set<string> };
const launches = globals.__topcardCardLaunches ??= new Set<string>();
type Dependencies = {
  sync: (state: CardQueue) => Promise<unknown>;
  launch: (kind: unknown, workspace: QueueWorkspace, resume?: HarnessSession) => Promise<HarnessSession>;
  kill: (id: string) => void;
};

// Only capture and commit hold the queue lock. A slow CLI/SSH preflight must
// not hold up reads, Pi cards, or launches belonging to other cards.
export async function launchCardHarness(id: string, action: LaunchAction, kind: unknown, deps: Dependencies): Promise<CardQueue> {
  if (launches.has(id)) throw new Error("此卡片正在启动，请等待启动结果");
  launches.add(id);
  let launched: HarnessSession | undefined;
  let committed = false;
  try {
    const captured = await withCardQueue(async state => {
      await deps.sync(state);
      const card = state.cards.find(item => item.id === id);
      if (!card) throw new Error("卡片已不存在");
      if (action === "harness_start") {
        if (card.session || card.harness) throw new Error("请选择空白卡片");
      } else {
        if (!card.harness || !["error", "exited"].includes(card.harness.state)) throw new Error("请先退出当前 CLI");
        if (action === "harness_reopen" && card.harness.providerSessionId) throw new Error("已识别原会话，请使用继续会话");
      }
      const workspace = state.workspaces?.find(item => item.id === card.workspaceId);
      if (!workspace) throw new Error("工作区不存在");
      return structuredClone({ card, workspace });
    });
    const previous = captured.card.harness;
    launched = await deps.launch(action === "harness_start" ? kind : previous!.kind, captured.workspace,
      action === "harness_start" || action === "harness_reopen" ? undefined : previous);
    const state = await withCardQueue(async state => {
      const card = state.cards.find(item => item.id === id);
      const workspace = state.workspaces?.find(item => item.id === captured.workspace.id);
      if (!card || !workspace || card.workspaceId !== captured.card.workspaceId || card.cwd !== captured.card.cwd
        || card.session?.id !== captured.card.session?.id || card.harness?.terminalId !== previous?.terminalId
        || card.archivedAt !== captured.card.archivedAt
        || workspace.kind !== captured.workspace.kind || workspace.cwd !== captured.workspace.cwd
        || workspace.runtimeCwd !== captured.workspace.runtimeCwd || workspace.sshHost !== captured.workspace.sshHost) {
        throw new Error("启动期间卡片或工作区已变更，请重试");
      }
      card.harness = launched;
      card.phase = "attention";
      delete card.archivedAt;
      if (!state.order.includes(id)) state.order.push(id);
      await deps.sync(state);
      return state;
    });
    committed = true;
    if (previous) deps.kill(previous.terminalId);
    return state;
  } finally {
    launches.delete(id);
    // Includes a failed queue write or a card removed while SSH was connecting.
    if (launched && !committed) deps.kill(launched.terminalId);
  }
}
