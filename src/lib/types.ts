export type SubagentSessionStatus =
  | "starting"
  | "running"
  | "completed"
  | "failed"
  | "aborted"
  | "interrupted";

export interface SessionTreeNode {
  entry: { id: string; type: string; parentId: string | null; timestamp: string };
  children: SessionTreeNode[];
  label?: string;
}

export interface SessionInfo {
  path: string;
  id: string;
  cwd: string;
  name?: string;
  created: string;
  modified: string;
  messageCount: number;
  firstMessage: string;
  parentSessionId?: string;
  relation?:
    | { kind: "fork"; originSessionId?: string }
    | {
        kind: "subagent";
        parentSessionId: string;
        profile: string;
        description: string;
        status: SubagentSessionStatus;
      };
  projectRoot?: string;
  projectKey?: string;
  branch?: string;
  isWorktree?: boolean;
  transient?: boolean;
}

export type ExtensionUiRequest = {
  id: string;
  method: "select" | "confirm" | "input" | "editor" | "custom" | string;
  closed?: boolean;
};

export type BlockingExtensionUiRequest = ExtensionUiRequest;
