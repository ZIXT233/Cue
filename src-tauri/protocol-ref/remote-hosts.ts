import { WorkspaceMachineError } from './workspace-machine-errors';
import { readFile, mkdir, writeFile, rename, glob } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, resolve, join } from 'node:path';
import { randomUUID } from 'node:crypto';
export interface RemoteHost { id: string; name: string; hostname: string; user?: string; port?: number; source: 'config' | 'web'; visible?: boolean; connected?: boolean; }
const file = () => join(process.env.TOPCARD_DATA_DIR || join(process.cwd(), '.topcard'), 'remote-hosts.json');
const visibilityFile = () => join(process.env.TOPCARD_DATA_DIR || join(process.cwd(), '.topcard'), 'remote-host-visibility.json');
export async function savedHosts(): Promise<RemoteHost[]> {
  try { return JSON.parse(await readFile(file(), 'utf8')); }
  catch (e) { if ((e as NodeJS.ErrnoException).code === 'ENOENT') return []; throw e; }
}
export async function configHosts(config = join(homedir(), '.ssh/config'), seen = new Set<string>()): Promise<RemoteHost[]> {
  if (seen.has(config) || seen.size > 64) return [];
  seen.add(config);
  let contents: string;
  try { contents = await readFile(config, 'utf8'); } catch (e) { if ((e as NodeJS.ErrnoException).code === 'ENOENT') return []; throw e; }
  const hosts: RemoteHost[] = [];
  let activeHosts: RemoteHost[] = [];
  for (const line of contents.split(/\r?\n/)) {
    const words = line.match(/"[^"]*"|'[^']*'|[^\s=]+/g)?.map(w => w.replace(/^['"]|['"]$/g, '')) ?? [];
    const directive = words[0]?.toLowerCase();
    if (directive === 'host') {
      activeHosts = [];
      for (const name of words.slice(1)) {
        if (name.startsWith('#')) break;
        if (/^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(name)) {
          const host = { id: name, name, hostname: name, source: 'config' as const };
          hosts.push(host); activeHosts.push(host);
        }
      }
    }
    else if (directive === 'hostname' && words[1]) activeHosts.forEach(host => { host.hostname = words[1]; });
    else if (directive === 'user' && words[1]) activeHosts.forEach(host => { host.user = words[1]; });
    else if (directive === 'port' && /^\d+$/.test(words[1] || '')) activeHosts.forEach(host => { host.port = Number(words[1]); });
    else if (directive === 'include') for (const pattern of words.slice(1)) {
      if (pattern.startsWith('#')) break;
      const expanded = resolve(homedir(), '.ssh', pattern.replace(/^~(?=\/|$)/, homedir()));
      for await (const path of glob(expanded)) hosts.push(...await configHosts(path, seen));
    }
  }
  return [...new Map(hosts.map(h => [h.id, h])).values()];
}
async function hiddenConfigHosts(): Promise<Set<string>> {
  try {
    const value: unknown = JSON.parse(await readFile(visibilityFile(), 'utf8'));
    return new Set(Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : []);
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code === 'ENOENT') return new Set();
    throw e;
  }
}
export async function listHosts(config?: string) {
  const [configured, hidden, saved] = await Promise.all([configHosts(config), hiddenConfigHosts(), savedHosts()]);
  return [...configured.map(host => ({ ...host, visible: !hidden.has(host.id) })), ...saved];
}
const state = globalThis as typeof globalThis & { __remoteHostsLock?: Promise<unknown> };
export function setConfigHostVisibility(id: string, visible: boolean, config?: string) {
  const task = (state.__remoteHostsLock ?? Promise.resolve()).catch(() => {}).then(async () => {
    if (!(await configHosts(config)).some(host => host.id === id)) throw new WorkspaceMachineError('HOST_READ_ONLY');
    const hidden = await hiddenConfigHosts();
    if (visible) hidden.delete(id); else hidden.add(id);
    await mkdir(dirname(visibilityFile()), { recursive: true });
    const temp = `${visibilityFile()}.${randomUUID()}.tmp`;
    await writeFile(temp, JSON.stringify([...hidden].sort(), null, 2), { mode: 0o600 });
    await rename(temp, visibilityFile());
  });
  state.__remoteHostsLock = task;
  return task;
}
export function updateHost(input: Partial<RemoteHost>, remove = false) {
  const task = (state.__remoteHostsLock ?? Promise.resolve()).catch(() => {}).then(async () => {
    const hosts = await savedHosts();
    if (remove) {
      if (!hosts.some(h => h.id === input.id)) throw new WorkspaceMachineError('HOST_READ_ONLY');
    } else {
      if (!input.name?.trim() || !input.hostname || !/^[a-zA-Z0-9][a-zA-Z0-9.:-]*$/.test(input.hostname)) throw new WorkspaceMachineError('HOST_INVALID');
      if (input.user && !/^[a-zA-Z0-9_][a-zA-Z0-9_.-]*$/.test(input.user)) throw new WorkspaceMachineError('USER_INVALID');
      if (input.port !== undefined && (!Number.isInteger(input.port) || input.port < 1 || input.port > 65535)) throw new WorkspaceMachineError('PORT_INVALID');
      if (input.id && !hosts.some(h => h.id === input.id)) throw new WorkspaceMachineError('HOST_READ_ONLY');
    }
    const id = input.id || `web-${randomUUID()}`;
    const next = hosts.filter(h => h.id !== id);
    if (!remove) next.push({ id, name: input.name!.trim(), hostname: input.hostname!, user: input.user || undefined, port: input.port, source: 'web' });
    await mkdir(dirname(file()), { recursive: true });
    const temp = `${file()}.${randomUUID()}.tmp`;
    await writeFile(temp, JSON.stringify(next, null, 2), { mode: 0o600 });
    await rename(temp, file());
    return next.find(h => h.id === id);
  });
  state.__remoteHostsLock = task;
  return task;
}
