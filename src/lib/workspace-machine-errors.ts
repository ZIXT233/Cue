/** Stable error codes are translated at render time, including after a locale change. */
export class WorkspaceMachineError extends Error {
  constructor(public code: string) { super(code); }
}
export function machineErrorKey(error: unknown): string {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : '';
  return `machines.error.${MACHINE_ERROR_CODES.includes(code) ? code : 'CONNECTION_FAILED'}`;
}

/**
 * What the user reads: the localized line, plus — when the backend carried it —
 * the remote's own words. A bare "无法读取目录" hides which path failed; the
 * detail ("bash: cd: /srv/app: No such file or directory") is the actual answer.
 * Errors without a machine code are already raw remote text and pass through.
 */
export function machineErrorText(error: unknown, t: (key: string) => string): string {
  if (!(error && typeof error === 'object')) return t(machineErrorKey(error));
  const { code, message, detail, raw } = error as { code?: string; message?: string; detail?: string; raw?: boolean };
  if (raw && !code && message) return message;
  const base = t(machineErrorKey(error));
  return detail ? `${base}（${detail}）` : base;
}
export const MACHINE_ERROR_CODES = ['HOST_TRUST_REQUIRED', 'HOST_KEY', 'AUTH_REQUIRED', 'CONNECTION_FAILED', 'SOCKET_PATH', 'TIMEOUT', 'REFUSED', 'HOST_NOT_FOUND', 'DIRECTORY', 'HOST_INVALID', 'HOST_DELETED', 'HOST_READ_ONLY', 'USER_INVALID', 'PORT_INVALID', 'PASSWORD_INVALID', 'LOCAL_PICKER', 'PATH_INVALID', 'REQUEST_FAILED'] as const as readonly string[];

/**
 * Whether this failure is the SSH layer asking for a secret: either the host
 * key needs confirmation (HOST_TRUST_REQUIRED, prompt carries the fingerprint)
 * or every keyless method was rejected and a password is the next move
 * (AUTH_REQUIRED). Both are answerable through the auth challenge dialog.
 */
export function needsSshSecret(error: unknown): boolean {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : '';
  return code === 'AUTH_REQUIRED' || code === 'HOST_TRUST_REQUIRED';
}
