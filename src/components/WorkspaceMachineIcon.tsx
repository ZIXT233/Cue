export function WorkspaceMachineIcon({ name, size = 18, connectionStatus, statusLabel }: { name: 'parent' | 'local' | 'remote' | 'folder' | 'folder-plus' | 'chevron' | 'plus' | 'edit' | 'close' | 'lock'; size?: number; connectionStatus?: 'connected' | 'disconnected'; statusLabel?: string }) {
  const paths = {
    parent: <path d="M12 20V4m-6 6 6-6 6 6" />,
    local: <><rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 21h8M12 16v5" /></>,
    remote: <><rect x="4" y="3" width="16" height="7" rx="2" /><rect x="4" y="14" width="16" height="7" rx="2" /><path d="M8 6.5h.01M8 17.5h.01M12 6.5h5M12 17.5h5" /></>,
    folder: <path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v10H3Z" />,
    'folder-plus': <><path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v10H3Z" /><path d="M17 11v6M14 14h6" /></>,
    chevron: <path d="m9 5 7 7-7 7" />,
    plus: <path d="M12 5v14M5 12h14" />,
    edit: <><path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4Z" /></>,
    close: <path d="m6 6 12 12M18 6 6 18" />,
    lock: <><rect x="5" y="10" width="14" height="11" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v3" /></>,
  };
  const icon = <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
  if (!connectionStatus) return icon;
  return <span className={`workspace-machine-status is-${connectionStatus}`} title={statusLabel} aria-label={statusLabel}>{icon}<i aria-hidden="true" /></span>;
}
