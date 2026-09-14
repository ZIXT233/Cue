export function FileExplorer(_props: {
  cwd: string;
  changesCollapsed?: boolean;
  refreshKey?: number;
  fileSearchOpen?: boolean;
  onFileSearchOpenChange?: (open: boolean) => void;
  onOpenFile?: (path: string, name?: string, options?: { modeHint?: "diff" }) => void;
}) {
  return <p className="cq-task-unavailable">Workspace files stay in the terminal card for now.</p>;
}
