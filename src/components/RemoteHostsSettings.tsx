"use client";
import { useEffect, useRef, useState } from 'react';
import { useI18n } from '@/hooks/useI18n';
import type { RemoteHost } from '@/lib/remote-hosts';
import { machineErrorKey } from '@/lib/workspace-machine-errors';
import { ConfigButton, ConfigDetail, ConfigDetailActions, ConfigDetailHeader, ConfigDetailHeaderInfo, ConfigDetailStack, ConfigDetailTitle, ConfigPanelShell } from './SettingsUi';
import { WorkspaceMachineIcon } from './WorkspaceMachineIcon';
import { SshAuthChallenge, useSshAuthChallenge } from './SshAuthChallenge';

export async function machineRequest(body: object, signal?: AbortSignal) {
  const response = await fetch('/api/workspace-machines', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body), signal });
  const data = await response.json();
  if (!response.ok) throw Object.assign(new Error(data.code || 'REQUEST_FAILED'), { code: data.code, prompt: data.prompt });
  return data;
}
export function RemoteHostEditor({ host, onSaved, onCancel, onBack, hideTitle = false }: { host?: RemoteHost; onSaved: (host: RemoteHost) => void; onCancel: () => void; onBack?: () => void; hideTitle?: boolean }) {
  const { t } = useI18n();
  const [name, setName] = useState(host?.name || '');
  const [hostname, setHostname] = useState(host?.hostname || '');
  const [user, setUser] = useState(host?.user || '');
  const [port, setPort] = useState(String(host?.port || 22));
  const [busy, setBusy] = useState(false), [error, setError] = useState<unknown>(null);
  const [testing, setTesting] = useState(false), [testPassed, setTestPassed] = useState(false);
  const [testError, setTestError] = useState<unknown>(null);
  const auth = useSshAuthChallenge();
  const testController = useRef<AbortController | null>(null);
  useEffect(() => () => testController.current?.abort(), []);
  const invalidateTest = () => { testController.current?.abort(); testController.current = null; setTesting(false); setTestPassed(false); setTestError(null); };
  const testConnection = async (password?: string, trustedPrompt?: string) => {
    testController.current?.abort();
    const controller = new AbortController();
    testController.current = controller;
    setTesting(true); setTestPassed(false); setTestError(null); setError(null);
    auth.clear();
    try { await machineRequest({ action: 'test', host: { hostname, user, port: Number(port) }, ...(password !== undefined ? { password } : {}), ...(trustedPrompt !== undefined ? { trustedPrompt } : {}) }, controller.signal); if (!controller.signal.aborted) setTestPassed(true); }
    catch (e) { if (!controller.signal.aborted) { setTestError(e); auth.present(e); } }
    finally { if (testController.current === controller) { testController.current = null; setTesting(false); } }
  };
  return <><form className="remote-host-editor" onSubmit={async e => {
    e.preventDefault(); invalidateTest(); setBusy(true); setError(null);
    try { const data = await machineRequest({ action: 'save', host: { id: host?.id, name, hostname, user, port: Number(port) } }); onSaved(data.host); }
    catch (e) { setError(e); } finally { setBusy(false); }
  }}>
    {!hideTitle && <h3>{t(host ? 'machines.edit' : 'machines.add')}</h3>}
    <label>{t('machines.name')}<input autoFocus required disabled={busy} value={name} onChange={e => setName(e.target.value)} placeholder={t('machines.namePlaceholder')} /></label>
    <div className="machine-field-grid"><label>{t('machines.hostname')}<input required disabled={busy} value={hostname} onChange={e => { invalidateTest(); setHostname(e.target.value); }} placeholder={t('machines.hostnamePlaceholder')} /></label>
    <label>{t('machines.port')}<input type="number" required disabled={busy} min="1" max="65535" value={port} onChange={e => { invalidateTest(); setPort(e.target.value); }} /></label></div>
    <label>{t('machines.user')}<input disabled={busy} value={user} onChange={e => { invalidateTest(); setUser(e.target.value); }} placeholder={t('machines.userPlaceholder')} /></label>
    {!!error && <p className="machine-error" role="alert">{t(machineErrorKey(error))}</p>}
    <div className={`machine-test-status${testError ? ' is-error' : testPassed ? ' is-success' : ''}`} role={testError ? 'alert' : 'status'} title={testError ? t(machineErrorKey(testError)) : undefined}>{testError ? t(machineErrorKey(testError)) : testPassed ? t('machines.connectionReady') : ''}</div>
    <div className="machine-actions"><ConfigButton aria-busy={testing || undefined} disabled={busy || testing || !hostname || !port} onClick={() => void testConnection()}>{t(testing ? 'machines.testingConnection' : 'machines.testConnection')}</ConfigButton><ConfigButton disabled={busy} onClick={() => { invalidateTest(); auth.clear(); if (onBack) onBack(); else onCancel(); }}>{onBack ? `← ${t('machines.back')}` : t('machines.cancel')}</ConfigButton><ConfigButton variant="primary" disabled={busy} type="submit">{t(busy ? 'machines.saving' : 'machines.saveHost')}</ConfigButton></div>
  </form><SshAuthChallenge challenge={auth.challenge} hostName={name || hostname} busy={testing} error={testError} onCancel={() => { invalidateTest(); auth.clear(); }} onRetry={(password, trustedPrompt) => void testConnection(password, trustedPrompt)} /></>;
}
export function RemoteHostsSettings() {
  const { t } = useI18n();
  const [hosts, setHosts] = useState<RemoteHost[]>([]), [error, setError] = useState<unknown>(null);
  const [editing, setEditing] = useState<RemoteHost | 'new' | null>(null), [busy, setBusy] = useState(false);
  const [testingHosts, setTestingHosts] = useState<ReadonlySet<string>>(() => new Set());
  const [visibilityHosts, setVisibilityHosts] = useState<ReadonlySet<string>>(() => new Set());
  const [hostTests, setHostTests] = useState<Record<string, { ok: boolean; errorKey?: string }>>({});
  const [authHost, setAuthHost] = useState<RemoteHost | null>(null);
  const [authError, setAuthError] = useState<unknown>(null);
  const auth = useSshAuthChallenge();
  const refresh = async () => { const r = await fetch('/api/workspace-machines'); const data = await r.json(); if (!r.ok) throw Object.assign(new Error(), { code: data.code }); setHosts(data.hosts); };
  const testHost = async (host: RemoteHost, password?: string, trustedPrompt?: string) => {
    setAuthHost(host); setAuthError(null); auth.clear();
    setTestingHosts(current => new Set(current).add(host.id));
    setHostTests(current => { const next = { ...current }; delete next[host.id]; return next; });
    try { await machineRequest({ action: 'test-host', host: host.id, ...(password !== undefined ? { password } : {}), ...(trustedPrompt !== undefined ? { trustedPrompt } : {}) }); setHostTests(current => ({ ...current, [host.id]: { ok: true } })); setHosts(current => current.map(item => item.id === host.id ? { ...item, connected: true } : item)); window.dispatchEvent(new Event('topcard-remote-hosts-changed')); setAuthHost(null); }
    catch (e) { setHosts(current => current.map(item => item.id === host.id ? { ...item, connected: false } : item)); window.dispatchEvent(new Event('topcard-remote-hosts-changed')); if (auth.present(e)) setAuthError(e); else { setHostTests(current => ({ ...current, [host.id]: { ok: false, errorKey: machineErrorKey(e) } })); setAuthHost(null); } }
    finally { setTestingHosts(current => { const next = new Set(current); next.delete(host.id); return next; }); }
  };
  const setHostVisibility = async (host: RemoteHost, visible: boolean) => {
    setVisibilityHosts(current => new Set(current).add(host.id));
    setHosts(current => current.map(item => item.id === host.id ? { ...item, visible } : item));
    setError(null);
    try {
      await machineRequest({ action: 'set-visibility', host: host.id, visible });
      window.dispatchEvent(new Event('topcard-remote-hosts-changed'));
    } catch (e) {
      setHosts(current => current.map(item => item.id === host.id ? { ...item, visible: !visible } : item));
      setError(e);
    } finally {
      setVisibilityHosts(current => { const next = new Set(current); next.delete(host.id); return next; });
    }
  };
  useEffect(() => {
    void refresh().catch(setError);
  }, []);
  return <><ConfigPanelShell embedded title={t('machines.settings')} onClose={() => {}}><ConfigDetail><ConfigDetailStack className="remote-host-settings">
    <ConfigDetailHeader><ConfigDetailHeaderInfo><ConfigDetailTitle>{t('machines.settings')}</ConfigDetailTitle></ConfigDetailHeaderInfo><ConfigDetailActions>{!editing && <ConfigButton size="small" onClick={() => setEditing('new')}><WorkspaceMachineIcon name="plus" size={14} />{t('machines.add')}</ConfigButton>}</ConfigDetailActions></ConfigDetailHeader>
    <p className="machine-help">{t('machines.settingsHint')}</p>
    {editing ? <RemoteHostEditor host={editing === 'new' ? undefined : editing} onCancel={() => setEditing(null)} onSaved={() => { setEditing(null); void refresh().catch(setError); }} /> : <div>
      {hosts.map(host => {
        const testResult = hostTests[host.id];
        const testErrorLabel = testResult?.errorKey ? t(testResult.errorKey) : undefined;
        const testing = testingHosts.has(host.id);
        const visibilityBusy = visibilityHosts.has(host.id);
        const target = `${host.user ? `${host.user}@` : ''}${host.hostname}${host.port && host.port !== 22 ? `:${host.port}` : ''}`;
        return <div className="machine-row" key={host.id}><WorkspaceMachineIcon name="remote" connectionStatus={host.connected ? 'connected' : 'disconnected'} statusLabel={t(host.connected ? 'machines.connected' : 'machines.disconnected')} /><span className="machine-row-text"><strong>{host.name}</strong><small>{target}{host.source === 'config' && <span className="machine-row-source"> · {t('machines.configSource')}</span>}</small></span><div className="machine-row-actions">{host.source === 'config' && <label className="machine-visibility-toggle"><input type="checkbox" checked={host.visible !== false} disabled={visibilityBusy} onChange={event => void setHostVisibility(host, event.target.checked)} /><span>{t('machines.showHost')}</span></label>}{testResult && <span className={`machine-row-test-status ${testResult.ok ? 'is-success' : 'is-error'}`} role="status" title={testErrorLabel}>{testResult.ok ? t('machines.testPassedShort') : t('machines.testFailedShort')}</span>}<ConfigButton size="small" variant="ghost" aria-busy={testing || undefined} disabled={busy || testing} onClick={() => void testHost(host)}>{t(testing ? 'machines.testingConnection' : 'machines.testConnection')}</ConfigButton>{host.source === 'web' ? <><ConfigButton size="small" variant="ghost" disabled={busy} onClick={() => setEditing(host)}>{t('machines.edit')}</ConfigButton><ConfigButton size="small" variant="ghost" disabled={busy} onClick={async () => { setBusy(true); setError(null); try { await machineRequest({ action: 'delete', id: host.id }); await refresh(); } catch (e) { setError(e); } finally { setBusy(false); } }}>{t('machines.delete')}</ConfigButton></> : <span className="machine-source">{t('machines.configSource')}</span>}</div></div>;
      })}
    </div>}
    {!!error && <p className="machine-error" role="alert">{t(machineErrorKey(error))}</p>}
  </ConfigDetailStack></ConfigDetail></ConfigPanelShell><SshAuthChallenge challenge={auth.challenge} hostName={authHost?.name || ''} busy={!!authHost && testingHosts.has(authHost.id)} error={authError} onCancel={() => { auth.clear(); setAuthHost(null); setAuthError(null); }} onRetry={(password, trustedPrompt) => { if (authHost) void testHost(authHost, password, trustedPrompt); }} /></>;
}
