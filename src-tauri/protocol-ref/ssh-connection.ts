import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:net';
import { WorkspaceMachineError } from './workspace-machine-errors';
import { savedHosts, type RemoteHost } from './remote-hosts';
import { embeddedNodeExecutable, embeddedNodeShellPrefix } from './node-runtime';
const exec = promisify(execFile);
const state = globalThis as typeof globalThis & { __sshSocketDirV2?: Promise<string> };
// OpenSSH appends a temporary suffix when binding. macOS TMPDIR alone can
// consume most of sockaddr_un.sun_path; use a short, private mkdtemp directory.
export const createSshTempDirectory = () => mkdtemp(join(process.platform === 'win32' ? tmpdir() : '/tmp', 'cq-s-'));
const socketDir = () => state.__sshSocketDirV2 ??= createSshTempDirectory();
function targetArgs(target: Pick<RemoteHost, 'hostname' | 'user' | 'port'>, socket: string, requestTty = false) {
  if (!target.hostname || !/^[a-zA-Z0-9][a-zA-Z0-9.:-]*$/.test(target.hostname)) throw new WorkspaceMachineError('HOST_INVALID');
  if (target.user && !/^[a-zA-Z0-9_][a-zA-Z0-9_.-]*$/.test(target.user)) throw new WorkspaceMachineError('USER_INVALID');
  if (target.port !== undefined && (!Number.isInteger(target.port) || target.port < 1 || target.port > 65535)) throw new WorkspaceMachineError('PORT_INVALID');
  return [requestTty ? '-tt' : '-T', '-S', socket, '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', '-o', 'ConnectTimeout=8', '-o', 'ServerAliveInterval=15', ...(target.port ? ['-p', String(target.port)] : []), ...(target.user ? ['-l', target.user] : []), target.hostname];
}
export async function transientConnectionArgs(target: Pick<RemoteHost, 'hostname' | 'user' | 'port'>) {
  const socket = join(await socketDir(), createHash('sha256').update(JSON.stringify(target)).digest('hex').slice(0, 24));
  return targetArgs(target, socket);
}
export async function connectionArgs(host: string, options?: { requestTty?: boolean }) {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._@:-]*$/.test(host)) throw new WorkspaceMachineError('HOST_INVALID');
  const saved = (await savedHosts()).find(h => h.id === host);
  if (host.startsWith('web-') && !saved) throw new WorkspaceMachineError('HOST_DELETED');
  const socket = join(await socketDir(), createHash('sha256').update(JSON.stringify(saved || host)).digest('hex').slice(0, 24));
  return targetArgs(saved || { hostname: host }, socket, options?.requestTty);
}
async function connectSshArgs(args: string[], password?: string, trustedPrompt?: string, signal?: AbortSignal) {
  // Existing authenticated masters are shared by directory browsing and Pi tools.
  try { await exec('ssh', ['-O', 'check', ...args], { timeout: 5000, signal }); return; } catch { if (signal?.aborted) throw new DOMException('Aborted', 'AbortError'); }
  let dir: string | undefined;
  let server: ReturnType<typeof createServer> | undefined;
  let challenge = "";
  try {
    const env = { ...process.env };
    {
      dir = await createSshTempDirectory();
      const socket = join(dir, 's');
      server = createServer(client => {
        client.setTimeout(5000, () => client.destroy());
        client.once('data', data => {
          const prompt = data.toString();
          if (/continue connecting[\s\S]*yes\/no/i.test(prompt)) {
            if (trustedPrompt === prompt) client.end('yes\n');
            else { challenge = prompt; client.end('no\n'); }
          } else if (/password|passphrase/i.test(prompt)) client.end((password ?? '') + '\n');
          else client.end('\n');
        });
      });
      await new Promise<void>((resolve, reject) => { server!.once('error', reject); server!.listen(socket, resolve); });
      const helper = join(dir, 'askpass');
      await writeFile(helper, `#!/bin/sh\nexec ${embeddedNodeShellPrefix()}'${embeddedNodeExecutable().replace(/'/g, `'"'"'`)}' -e 'const c=require("net").connect(process.env.CQ_ASK_SOCKET,()=>c.write(process.argv[1]));c.pipe(process.stdout)' "$1"\n`, { mode: 0o700 });
      Object.assign(env, { SSH_ASKPASS: helper, SSH_ASKPASS_REQUIRE: 'force', DISPLAY: ':0', CQ_ASK_SOCKET: socket });
      args[args.indexOf('BatchMode=yes')] = 'BatchMode=no';
      args[args.indexOf('StrictHostKeyChecking=yes')] = 'StrictHostKeyChecking=ask';
    }
    await exec('ssh', ['-M', '-N', '-f', '-o', 'ControlPersist=8h', '-o', 'NumberOfPasswordPrompts=1', ...args], { env, timeout: 25000, maxBuffer: 1024 * 1024, signal });
  } catch (e) {
    if (challenge) throw Object.assign(new Error(challenge), { code: "HOST_TRUST_REQUIRED", prompt: challenge });
    throw e;
  } finally {
    server?.close();
    if (dir) await rm(dir, { recursive: true, force: true });
  }
}
export async function connectSsh(host: string, password?: string, trustedPrompt?: string, signal?: AbortSignal) {
  return connectSshArgs(await connectionArgs(host), password, trustedPrompt, signal);
}
export async function isSshConnected(host: string) {
  try {
    await exec('ssh', ['-O', 'check', ...await connectionArgs(host)], { timeout: 5000 });
    return true;
  } catch {
    return false;
  }
}
export async function testSshConnection(target: Pick<RemoteHost, 'hostname' | 'user' | 'port'>, password?: string, trustedPrompt?: string, signal?: AbortSignal) {
  return connectSshArgs(await transientConnectionArgs(target), password, trustedPrompt, signal);
}
export function sshError(error: unknown) {
  if (error instanceof WorkspaceMachineError) return { error: error.code, code: error.code };
  if ((error as { code?: string })?.code === "HOST_TRUST_REQUIRED") return { error: "HOST_TRUST_REQUIRED", code: "HOST_TRUST_REQUIRED", prompt: (error as { prompt: string }).prompt };
  const message = error instanceof Error ? error.message : String(error);
  const code = /too long for Unix domain socket|ENAMETOOLONG|unix_listener/i.test(message) ? 'SOCKET_PATH'
    : /Host key verification failed|REMOTE HOST IDENTIFICATION HAS CHANGED/i.test(message) ? 'HOST_KEY'
    : /Permission denied|incorrect passphrase|sign_and_send_pubkey/i.test(message) ? 'AUTH_REQUIRED'
    : /timed out|ETIMEDOUT/i.test(message) || (error as { killed?: boolean })?.killed ? 'TIMEOUT'
    : /Connection refused/i.test(message) ? 'REFUSED'
    : /Could not resolve hostname/i.test(message) ? 'HOST_NOT_FOUND'
    : /can't cd|cannot cd|No such file or directory|Not a directory/i.test(message) ? 'DIRECTORY'
    : 'CONNECTION_FAILED';
  // Never expose execFile's command line, temporary paths or raw stderr in UI.
  return { error: code, code };
}
