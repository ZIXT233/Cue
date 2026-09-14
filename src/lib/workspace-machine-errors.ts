/** Stable error codes are translated at render time, including after a locale change. */
export class WorkspaceMachineError extends Error {
  constructor(public code: string) { super(code); }
}
export function machineErrorKey(error: unknown): string {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : '';
  return `machines.error.${MACHINE_ERROR_CODES.includes(code) ? code : 'CONNECTION_FAILED'}`;
}
export const MACHINE_ERROR_CODES = ['HOST_TRUST_REQUIRED', 'HOST_KEY', 'AUTH_REQUIRED', 'CONNECTION_FAILED', 'SOCKET_PATH', 'TIMEOUT', 'REFUSED', 'HOST_NOT_FOUND', 'DIRECTORY', 'HOST_INVALID', 'HOST_DELETED', 'HOST_READ_ONLY', 'USER_INVALID', 'PORT_INVALID', 'PASSWORD_INVALID', 'LOCAL_PICKER', 'PATH_INVALID', 'REQUEST_FAILED'] as const as readonly string[];
