import { WorkspaceMachineError } from '@/lib/workspace-machine-errors';
import { NextResponse } from 'next/server';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { listHosts, setConfigHostVisibility, updateHost } from '@/lib/remote-hosts';
import { connectSsh, isSshConnected, sshError, testSshConnection } from '@/lib/ssh-connection';
import { sshExec, shellQuote } from '@/lib/ssh-workspace';
const exec = promisify(execFile);
export const runtime = 'nodejs';
export async function GET() {
  try {
    const hosts = await listHosts();
    return NextResponse.json({ hosts: await Promise.all(hosts.map(async host => ({ ...host, connected: await isSshConnected(host.id) }))) }, { headers: { 'Cache-Control': 'no-store' } });
  }
  catch (e) { return NextResponse.json(sshError(e), { status: 500 }); }
}
export async function POST(req: Request) {
  try {
    const body = await req.json();
    if (body.action === 'save') return NextResponse.json({ host: await updateHost(body.host) });
    if (body.action === 'delete') { await updateHost({ id: body.id }, true); return NextResponse.json({ ok: true }); }
    if (body.action === 'set-visibility') {
      if (typeof body.host !== 'string' || typeof body.visible !== 'boolean') throw new WorkspaceMachineError('HOST_INVALID');
      await setConfigHostVisibility(body.host, body.visible);
      return NextResponse.json({ ok: true });
    }
    if (body.action === 'test') {
      const target = body.host && typeof body.host === 'object' ? body.host : {};
      if (body.password !== undefined && (typeof body.password !== 'string' || body.password.length > 8192 || /[\r\n\0]/.test(body.password))) throw new WorkspaceMachineError('PASSWORD_INVALID');
      await testSshConnection({ hostname: target.hostname, user: target.user || undefined, port: Number(target.port || 22) }, body.password, typeof body.trustedPrompt === 'string' ? body.trustedPrompt : undefined, req.signal);
      return NextResponse.json({ ok: true });
    }
    if (body.action === 'local-folder') {
      let cwd = '';
      const title = body.locale === 'zh-CN' ? '选择工作区目录' : body.locale === 'zh-TW' ? '選擇工作區目錄' : 'Choose workspace folder';
      try {
        if (process.platform === 'darwin') cwd = (await exec('osascript', ['-e', `POSIX path of (choose folder with prompt "${title}")`], { timeout: 120000 })).stdout.trim();
        else if (process.platform === 'win32') cwd = (await exec('powershell.exe', ['-NoProfile', '-STA', '-Command', 'Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; if ($dialog.ShowDialog() -eq "OK") { $dialog.SelectedPath }'], { timeout: 120000 })).stdout.trim();
        else cwd = (await exec('zenity', ['--file-selection', '--directory', `--title=${title}`], { timeout: 120000 })).stdout.trim();
      } catch (e) {
        if (/\(-128\)/.test(String(e)) || (e as { code?: number }).code === 1) return NextResponse.json({ cancelled: true });
        throw new WorkspaceMachineError('LOCAL_PICKER');
      }
      return NextResponse.json(cwd ? { cwd } : { cancelled: true });
    }
    if (typeof body.host !== 'string' || !(await listHosts()).some(h => h.id === body.host)) throw new WorkspaceMachineError('HOST_INVALID');
    if (body.action === 'test-host') {
      if (body.password !== undefined && (typeof body.password !== 'string' || body.password.length > 8192 || /[\r\n\0]/.test(body.password))) throw new WorkspaceMachineError('PASSWORD_INVALID');
      await connectSsh(body.host, body.password, typeof body.trustedPrompt === 'string' ? body.trustedPrompt : undefined, req.signal);
      return NextResponse.json({ ok: true });
    }
    if (body.action === 'connect') {
      if (body.password !== undefined && (typeof body.password !== 'string' || body.password.length > 8192 || /[\r\n\0]/.test(body.password))) throw new WorkspaceMachineError('PASSWORD_INVALID');
      await connectSsh(body.host, body.password, typeof body.trustedPrompt === "string" ? body.trustedPrompt : undefined, req.signal);
      const cwd = (await sshExec(body.host, 'printf "%s" "$HOME"')).toString();
      return NextResponse.json({ cwd });
    }
    if (body.action === 'directories') {
      if (typeof body.path !== 'string' || !body.path.startsWith('/') || body.path.includes('\0')) throw new WorkspaceMachineError('PATH_INVALID');
      // Prefix completion: /home/u lists children of /home; /home/u/ lists u's children.
      const slash = body.path.lastIndexOf('/');
      const parent = body.path.slice(0, slash + 1), prefix = body.path.slice(slash + 1);
      const output = await sshExec(body.host, `cd ${shellQuote(parent)} && for p in ./* ./.[!.]* ./..?*; do [ -d "$p" ] && printf '%s\\0' "$p"; done; true`);
      const entries = output.toString().split('\0').filter(Boolean).map(p => p.slice(2)).filter(p => p.startsWith(prefix)).sort((a, b) => a.localeCompare(b));
      return NextResponse.json({ directories: entries.slice(0, 300).map(name => ({ name, path: parent + name + '/' })), truncated: entries.length > 300 });
    }
    throw new WorkspaceMachineError('REQUEST_FAILED');
  } catch (e) { return NextResponse.json(sshError(e), { status: 400 }); }
}
