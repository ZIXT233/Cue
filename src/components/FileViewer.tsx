export function FileViewer(_props: {
  filePath: string;
  cwd?: string | null;
  sourceSessionId?: string;
  initialDisplayMode?: string;
  gitRefreshKey?: number;
  onOpenFile?: (path: string) => void;
}) {
  return <p className="cq-task-unavailable">File preview is not part of Cue yet. Open the path from the terminal card.</p>;
}
