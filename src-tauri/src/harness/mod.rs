mod adapters;
mod codex;
pub(crate) mod codex_session;
pub(crate) mod cursor_session;
pub(crate) mod claude_session;
mod debug;
pub(crate) mod pi_session;
pub(crate) mod omp_session;
mod grok_session;
mod label_text;
mod session_find;
mod session_label;
mod env;
mod hooks;
mod inherited;
mod notify_osc;
mod osc;
mod shell;
mod signals;
mod windows;

pub use adapters::adapter;
pub use debug::HarnessDebugSnapshot;
pub use session_label::session_exists;
pub use env::local_environment;
pub use hooks::prepare_hook_launch;
pub use osc::HookOscProbe;
pub use shell::prepare_shell;
pub use signals::{observe_hook, observe_title, HookSignal, ProbeState};
pub use windows::windows_command;

use crate::error::{AppError, AppResult};
use crate::live::LiveBus;
use crate::models::{HarnessSession, QueueWorkspace};
use crate::paths::signal_dir;
use crate::settings::SettingsStore;
use crate::ssh::{connection_args, ssh_login_command, ssh_login_exec};
use crate::terminal::TerminalHub;
use crate::transcript::read_terminal_transcript;
use notify_osc::{notify_osc_kinds, observe_notify, prefer_kitty_notifications, KittyNotifyProbe};
use std::panic::{catch_unwind, AssertUnwindSafe};
use codex_session::{codex_exit_session_id, resolve_codex_session_prefix};
use session_label::refresh_probe_label;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone)]
pub struct HarnessRuntime {
    probes: Arc<Mutex<HashMap<String, ProbeState>>>,
    debug: Arc<debug::DebugLog>,
    live: LiveBus,
}

impl HarnessRuntime {
    pub fn new(live: LiveBus) -> Self {
        let probes = Arc::new(Mutex::new(HashMap::new()));
        let debug = Arc::new(Mutex::new(HashMap::new()));
        start_signal_watch(probes.clone(), debug.clone(), live.clone());
        Self { probes, debug, live }
    }

    pub fn debug_snapshot(&self, terminal_id: &str, terminals: &TerminalHub) -> HarnessDebugSnapshot {
        self.drain_signals(terminal_id);
        debug::snapshot(&self.debug, &self.probes, terminals, terminal_id)
    }

    pub fn snapshot(&self, session: &HarnessSession, terminals: &TerminalHub) -> HarnessSession {
        let mut current = self.probes.lock().get(&session.terminal_id).cloned();
        if current.is_some() {
            self.drain_signals(&session.terminal_id);
            current = self.probes.lock().get(&session.terminal_id).cloned();
        }
        let mut provider_session_id = current.as_ref().and_then(|c| c.session_id.clone()).or_else(|| session.provider_session_id.clone());
        if session.kind == "codex" {
            if let Some(prefix) = current.as_ref().and_then(|c| c.session_id_prefix.as_deref()) {
                let prefix = prefix.to_lowercase();
                provider_session_id = if session.remote == Some(true) {
                    provider_session_id.filter(|id| id.to_lowercase().starts_with(&prefix))
                } else {
                    resolve_codex_session_prefix(&prefix)
                };
            }
        }
        let terminal = terminals.snapshot(&session.terminal_id);
        let dead = terminal.as_ref().is_none_or(|t| t.exited);
        let mut unpersisted_session = None;
        if dead && session.remote != Some(true) {
            if session.kind == "codex" {
                let output = terminal.as_ref().map(|t| t.output.clone())
                    .or_else(|| read_terminal_transcript(&session.terminal_id).map(|saved| saved.output))
                    .unwrap_or_default();
                if let Some(footer_id) = codex_exit_session_id(&output) {
                    let persisted = session_exists("codex", &footer_id);
                    provider_session_id = if persisted == Some(true) { Some(footer_id) } else { None };
                    unpersisted_session = Some(persisted == Some(false));
                }
            }
            if let Some(id) = provider_session_id.as_deref() {
                if session_exists(&session.kind, id) == Some(false) {
                    provider_session_id = None;
                    unpersisted_session = Some(true);
                }
            }
        }
        let session_name = current.as_ref().and_then(|c| c.session_name.clone()).or_else(|| session.session_name.clone());
        let first_prompt = current.as_ref().and_then(|c| c.first_prompt.clone()).or_else(|| session.first_prompt.clone());
        let submit_prompt = current.as_ref().and_then(|c| c.submit_prompt.clone()).or_else(|| session.submit_prompt.clone());
        let shell_notify = session.kind == "shell"
            && session.shell_notify == Some(true)
            && session.shell_command_started_at == current.as_ref().and_then(|c| c.shell_command_started_at);
        let state = match &terminal {
            None => "error".into(),
            Some(t) if t.exited => "exited".into(),
            Some(_) if session.kind == "shell" => {
                if shell_notify && current.as_ref().and_then(|c| c.shell_command_running) == Some(true) {
                    "working".into()
                } else {
                    "attention".into()
                }
            }
            Some(_) => current.as_ref().map(|c| c.state.clone()).unwrap_or_else(|| "unknown".into()),
        };
        let mut next = session.clone();
        if session.kind == "shell" {
            next.shell_command_started_at = current.as_ref().and_then(|c| c.shell_command_started_at);
            next.shell_command_running = current.as_ref().and_then(|c| c.shell_command_running);
            next.shell_exit_code = current.as_ref().and_then(|c| c.shell_exit_code);
            next.shell_notify = Some(shell_notify);
        }
        next.state = state;
        next.provider_session_id = provider_session_id.clone();
        next.unpersisted_session = unpersisted_session;
        next.reply_preview = current.as_ref().and_then(|c| c.reply_preview.clone());
        next.session_name = session_name;
        next.first_prompt = first_prompt;
        next.submit_prompt = submit_prompt;
        next.title = None;
        next.source = current.as_ref().and_then(|c| c.source.clone());
        next.probe = Some(match current.as_ref() {
            Some(c) if c.hook_seen && c.title_seen => "hooks-and-title".into(),
            Some(c) if c.hook_seen => "hooks".into(),
            Some(c) if c.title_seen => "title-only".into(),
            _ => "unconfirmed".into(),
        });
        if let Some(code) = terminal.as_ref().and_then(|t| t.exit_code) {
            next.exit_code = Some(code);
        }
        next
    }

    fn drain_signals(&self, terminal_id: &str) {
        let directory = signal_dir(terminal_id);
        let Ok(entries) = std::fs::read_dir(&directory) else { return };
        let mut files: Vec<_> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| regex::Regex::new(r"^\d+-[a-f0-9-]+\.json$").unwrap().is_match(name))
            })
            .collect();
        files.sort();
        apply_file_signals(&self.probes, &self.debug, terminal_id);
    }

    pub async fn launch(
        &self,
        kind: &str,
        workspace: &QueueWorkspace,
        resume: Option<HarnessSession>,
        terminals: &TerminalHub,
        settings: &SettingsStore,
        bin_dir: &std::path::Path,
    ) -> AppResult<HarnessSession> {
        let adapter = adapter(kind)?;
        let terminal_id = Uuid::new_v4().simple().to_string();
        let signals = signal_dir(&terminal_id);
        if resume.as_ref().is_some_and(|s| s.provider_session_id.is_none()) {
            return Err(AppError::msg("未捕获到原会话 ID，无法续接。请通过新会话入口创建新卡片。"));
        }
        let executable;
        let mut args: Vec<String>;
        let version;
        let mut env = HashMap::new();
        let mut shell_notifications = false;

        if adapter.id == "shell" {
            let shell = prepare_shell(workspace, &signals, bin_dir, settings.read()?.powershell_enabled).await?;
            executable = shell.executable;
            args = shell.args;
            version = shell.version;
            env = shell.env;
            shell_notifications = shell.command_notifications;
        } else {
            let mut command_path = adapter.executable.to_string();
            let command_prefix = Vec::new();
            if workspace.kind == "local" {
                let mut local = local_environment(false).await.unwrap_or_default();
                if let Some(resolved) = env::resolve_local_command(&adapter.executable, &local) {
                    command_path = resolved;
                    env = local;
                } else {
                    local = local_environment(true).await.unwrap_or_default();
                    if let Some(resolved) = env::resolve_local_command(&adapter.executable, &local) {
                        command_path = resolved;
                        env = local;
                    } else {
                        return Err(AppError::msg(format!("找不到 {}：已读取用户 Shell 环境并检查常见安装目录", adapter.executable)));
                    }
                }
            }
            let version_flag = if adapter.id == "grok" { "version" } else { "--version" };
            version = detect_version(workspace, &command_path, &command_prefix, version_flag, &env).await?;
            if adapter.id == "pi" && !pi_version_ok(&version) {
                return Err(AppError::msg("Pi CLI 状态集成需要 Pi 0.80.4 或更新版本（agent_settled 事件），请先升级机器上的 Pi"));
            }
            let hooks = prepare_hook_launch(adapter.id, &signals, workspace, &terminal_id, bin_dir).await?;
            env.extend(hooks.env);
            if adapter.id == "cursor" {
                prefer_kitty_notifications(&mut env);
            }
            if let Some(session_id) = resume.as_ref().and_then(|s| s.provider_session_id.clone()) {
                env.insert("CUE_HARNESS_SESSION_ID".into(), session_id);
            }
            let mut launch_args = command_prefix;
            if let Some(session_id) = resume.as_ref().and_then(|s| s.provider_session_id.as_deref()) {
                launch_args.extend(adapter.resume_args(session_id)?);
            }
            launch_args.extend(adapter.args.iter().map(|s| s.to_string()));
            launch_args.extend(hooks.args);
            if workspace.kind == "ssh" {
                let host = workspace.ssh_host.as_deref().ok_or_else(|| AppError::msg("工作区不存在"))?;
                let exports = env.iter().map(|(k, v)| format!("{k}={}", crate::ssh::shell_quote(v))).collect::<Vec<_>>().join(" ");
                let command = std::iter::once(adapter.executable.to_string()).chain(launch_args).map(|s| crate::ssh::shell_quote(&s)).collect::<Vec<_>>().join(" ");
                let remote = format!(
                    "cd {} && {}{}CUE_HARNESS_TTY=$(tty) && export CUE_HARNESS_TTY && exec {}",
                    crate::ssh::shell_quote(&workspace.cwd),
                    if adapter.id == "cursor" { "unset GHOSTTY_RESOURCES_DIR && " } else { "" },
                    if exports.is_empty() { String::new() } else { format!("export {exports} && ") },
                    command
                );
                executable = "ssh".into();
                args = connection_args(host, true).await?;
                args.push(ssh_login_command(&remote));
                env.clear();
            } else if cfg!(windows) {
                let launch = windows_command(&command_path, &launch_args);
                executable = launch.executable;
                args = launch.args;
            } else {
                executable = command_path;
                args = launch_args;
            }
        }

        let kind = adapter.id.to_string();
        self.probes.lock().insert(terminal_id.clone(), ProbeState {
            kind: Some(kind.clone()),
            remote: workspace.kind == "ssh",
            state: if kind == "shell" { "attention".into() } else { "starting".into() },
            at: now_ms(),
            session_id: resume.as_ref().and_then(|s| s.provider_session_id.clone()),
            session_name: resume.as_ref().and_then(|s| s.session_name.clone()),
            first_prompt: resume.as_ref().and_then(|s| s.first_prompt.clone()),
            submit_prompt: resume.as_ref().and_then(|s| s.submit_prompt.clone()),
            ..ProbeState::default()
        });
        let osc = Arc::new(std::sync::Mutex::new(HookOscProbe::new(terminal_id.clone())));
        let notify = if kind != "shell" {
            Some(Arc::new(std::sync::Mutex::new(KittyNotifyProbe::new())))
        } else {
            None
        };
        let title_probe = if kind == "codex" {
            Some(Arc::new(std::sync::Mutex::new(codex::CodexTitleProbe::new())))
        } else {
            None
        };
        let probes = self.probes.clone();
        let debug_log = self.debug.clone();
        let live = self.live.clone();
        let kind_cb = kind.clone();
        let id_cb = terminal_id.clone();
        let on_output: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |data: &str| {
            if kind_cb != "shell" {
                for kind in notify_osc_kinds(data) {
                    let mut map = probes.lock();
                    if let Some(current) = map.get_mut(&id_cb) {
                        current.notify_osc_hits = current.notify_osc_hits.saturating_add(1);
                        let first = !current.notify_osc_seen.iter().any(|seen| seen == kind);
                        if first {
                            current.notify_osc_seen.push(kind.to_string());
                        }
                        debug::record(&debug_log, &id_cb, debug::HarnessDebugEvent {
                            at: now_ms(),
                            source: "notify-osc".into(),
                            event: kind.into(),
                            state: current.state.clone(),
                            session_id: current.session_id.clone(),
                            prompt: None,
                            note: Some(if first { "seen-in-stream".into() } else { format!("seen-in-stream#{}", current.notify_osc_hits) }),
                        });
                    }
                }
            }
            let hook_ok = catch_unwind(AssertUnwindSafe(|| {
                if let Ok(mut osc) = osc.lock() {
                    if let Some(signal) = osc.push(data) {
                        if kind_cb != "shell" {
                            let mut map = probes.lock();
                            if let Some(current) = map.get(&id_cb).cloned() {
                                let mut next = observe_hook(current.clone(), signal.clone());
                                refresh_probe_label(&mut next);
                                debug::record(&debug_log, &id_cb, debug::HarnessDebugEvent {
                                    at: signal.at,
                                    source: "osc".into(),
                                    event: signal.event.clone(),
                                    state: next.state.clone(),
                                    session_id: signal.session_id.clone(),
                                    prompt: signal.prompt.clone(),
                                    note: None,
                                });
                                let changed = next.state != current.state || next.hook_seen != current.hook_seen
                                    || next.session_name != current.session_name || next.first_prompt != current.first_prompt
                                    || next.submit_prompt != current.submit_prompt;
                                map.insert(id_cb.clone(), next);
                                if changed { live.notify("hook"); }
                            }
                        }
                    }
                }
            }));
            if hook_ok.is_err() {
                debug::record(&debug_log, &id_cb, debug::HarnessDebugEvent {
                    at: now_ms(),
                    source: "probe".into(),
                    event: "panic".into(),
                    state: String::new(),
                    session_id: None,
                    prompt: None,
                    note: Some("hook-osc".into()),
                });
            }
            if let Some(notify) = &notify {
                let notify_ok = catch_unwind(AssertUnwindSafe(|| {
                    if let Ok(mut notify) = notify.lock() {
                        if let Some(signal) = notify.push(data) {
                            let mut map = probes.lock();
                            if let Some(current) = map.get(&id_cb).cloned() {
                                let next = observe_notify(current.clone(), &signal, now_ms());
                                debug::record(&debug_log, &id_cb, debug::HarnessDebugEvent {
                                    at: now_ms(),
                                    source: "notify-osc".into(),
                                    event: "Notification".into(),
                                    state: next.state.clone(),
                                    session_id: current.session_id.clone(),
                                    prompt: signal.body.clone(),
                                    note: Some(signal.id),
                                });
                                let changed = next.state != current.state || next.reply_preview != current.reply_preview;
                                map.insert(id_cb.clone(), next);
                                if changed { live.notify("hook"); }
                            }
                        }
                    }
                }));
                if notify_ok.is_err() {
                    debug::record(&debug_log, &id_cb, debug::HarnessDebugEvent {
                        at: now_ms(),
                        source: "probe".into(),
                        event: "panic".into(),
                        state: String::new(),
                        session_id: None,
                        prompt: None,
                        note: Some("notify-osc".into()),
                    });
                }
            }
            if let Some(probe) = &title_probe {
                if let Ok(mut probe) = probe.lock() {
                    let state = probe.push(data);
                    let needs_input = probe.consume_needs_input();
                    if state.is_some() || needs_input {
                        let mut map = probes.lock();
                        if let Some(current) = map.get(&id_cb).cloned() {
                            let raised = if needs_input {
                                observe_hook(current.clone(), HookSignal {
                                    event: "PermissionRequest".into(),
                                    at: now_ms(),
                                    session_id: current.session_id.clone(),
                                    ..Default::default()
                                })
                            } else {
                                current.clone()
                            };
                            let title_is_fallback = kind_cb != "codex" || !raised.hook_seen;
                            let mut next = observe_title(
                                raised.clone(),
                                state.as_deref().unwrap_or(raised.state.as_str()),
                                now_ms(),
                                kind_cb == "codex",
                            );
                            if title_is_fallback {
                                if probe.session_id.is_some() || probe.session_id_prefix.is_some() {
                                    next.session_id = probe.session_id.clone().or(next.session_id);
                                    next.session_id_prefix = probe.session_id_prefix.clone();
                                    next.identity_at = Some(now_ms());
                                }
                            }
                            debug::record(&debug_log, &id_cb, debug::HarnessDebugEvent {
                                at: now_ms(),
                                source: "title".into(),
                                event: if needs_input { "PermissionRequest".into() } else { state.clone().unwrap_or_else(|| "title".into()) },
                                state: next.state.clone(),
                                session_id: next.session_id.clone(),
                                prompt: None,
                                note: Some(if needs_input { "osc9-or-action-required".into() } else { "codex-title".into() }),
                            });
                            let changed = next.state != current.state || next.hook_seen != current.hook_seen;
                            map.insert(id_cb.clone(), next);
                            if changed { live.notify("hook"); }
                        }
                    }
                }
            }
            if kind_cb == "shell" && shell_notifications {
                if let Some((running, exit_code)) = shell::probe_chunk(data) {
                    let mut map = probes.lock();
                    if let Some(current) = map.get_mut(&id_cb) {
                        current.shell_command_running = Some(running);
                        if running { current.shell_command_started_at = Some(now_ms()); }
                        if !running { current.shell_exit_code = exit_code; }
                        live.notify("shell");
                    }
                }
            }
        });

        terminals.create(
            workspace.runtime_cwd.clone(),
            100,
            30,
            Some(terminal_id.clone()),
            executable,
            args,
            env,
            true,
            Some(on_output),
        )?;

        Ok(HarnessSession {
            kind,
            terminal_id,
            state: if adapter.id == "shell" { "attention".into() } else { "starting".into() },
            version,
            reply_preview: None,
            shell_command_notifications: Some(shell_notifications),
            shell_command_started_at: None,
            shell_command_running: None,
            shell_notify: None,
            shell_exit_code: None,
            exit_code: None,
            provider_session_id: resume.as_ref().and_then(|s| s.provider_session_id.clone()),
            session_name: resume.as_ref().and_then(|s| s.session_name.clone()).or_else(|| {
                if adapter.id == "shell" { Some(workspace.name.clone()) } else { None }
            }),
            first_prompt: resume.as_ref().and_then(|s| s.first_prompt.clone()),
            submit_prompt: resume.as_ref().and_then(|s| s.submit_prompt.clone()),
            title: None,
            unpersisted_session: None,
            remote: Some(workspace.kind == "ssh"),
            source: None,
            probe: None,
        })
    }
}

async fn detect_version(
    workspace: &QueueWorkspace,
    command_path: &str,
    prefix: &[String],
    flag: &str,
    env: &HashMap<String, String>,
) -> AppResult<String> {
    if workspace.kind == "ssh" {
        let host = workspace.ssh_host.as_deref().ok_or_else(|| AppError::msg("工作区不存在"))?;
        let cmd = format!("{} {}", command_path.rsplit(['/', '\\']).next().unwrap_or(command_path), flag);
        let out = ssh_login_exec(host, &cmd).await?;
        return Ok(String::from_utf8_lossy(&out).trim().to_string());
    }
    let mut argv: Vec<String> = prefix.to_vec();
    argv.push(flag.to_string());
    let (program, args) = if cfg!(windows) {
        let launch = windows_command(command_path, &argv);
        (launch.executable, launch.args)
    } else {
        (command_path.to_string(), argv)
    };
    let mut command = tokio::process::Command::new(program);
    command.args(args);
    command.current_dir(&workspace.cwd);
    for (key, value) in env { command.env(key, value); }
    let output = command.output().await.map_err(|e| AppError::msg(format!("启动检测失败：{e}")))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn pi_version_ok(version: &str) -> bool {
    let re = regex::Regex::new(r"(?:^|\s)v?(\d+)\.(\d+)\.(\d+)").unwrap();
    let Some(caps) = re.captures(version) else { return false };
    let major: u32 = caps[1].parse().unwrap_or(0);
    let minor: u32 = caps[2].parse().unwrap_or(0);
    let patch: u32 = caps[3].parse().unwrap_or(0);
    major > 0 || minor > 80 || (minor == 80 && patch >= 4)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn apply_file_signals(
    probes: &Mutex<HashMap<String, ProbeState>>,
    debug_log: &debug::DebugLog,
    terminal_id: &str,
) -> bool {
    let directory = signal_dir(terminal_id);
    let Ok(entries) = std::fs::read_dir(&directory) else { return false };
    let mut files: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| regex::Regex::new(r"^\d+-[a-f0-9-]+\.json$").unwrap().is_match(name))
        })
        .collect();
    files.sort();
    let mut changed = false;
    for path in files.into_iter().take(200) {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(signal) = serde_json::from_str::<HookSignal>(&raw) {
                let mut map = probes.lock();
                if let Some(current) = map.get(terminal_id).cloned() {
                    let mut next = observe_hook(current.clone(), signal.clone());
                    refresh_probe_label(&mut next);
                    debug::record(debug_log, terminal_id, debug::HarnessDebugEvent {
                        at: signal.at,
                        source: "file".into(),
                        event: signal.event.clone(),
                        state: next.state.clone(),
                        session_id: signal.session_id.clone(),
                        prompt: signal.prompt.clone(),
                        note: None,
                    });
                    if next.state != current.state || next.hook_seen != current.hook_seen
                        || next.session_name != current.session_name || next.first_prompt != current.first_prompt
                        || next.submit_prompt != current.submit_prompt
                    {
                        changed = true;
                    }
                    map.insert(terminal_id.to_string(), next);
                }
            }
        }
        let _ = std::fs::remove_file(path);
    }
    changed
}

fn start_signal_watch(
    probes: Arc<Mutex<HashMap<String, ProbeState>>>,
    debug_log: Arc<debug::DebugLog>,
    live: LiveBus,
) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(250));
        let ids: Vec<String> = probes.lock().keys().cloned().collect();
        let mut changed = false;
        for id in ids {
            if apply_file_signals(&probes, &debug_log, &id) {
                changed = true;
            }
        }
        if changed {
            live.notify("hook");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::HarnessSession;

    fn session(state: &str) -> HarnessSession {
        HarnessSession {
            kind: "codex".into(),
            terminal_id: "gone".into(),
            state: state.into(),
            version: String::new(),
            reply_preview: None,
            shell_command_notifications: None,
            shell_command_started_at: None,
            shell_command_running: None,
            shell_notify: None,
            shell_exit_code: None,
            exit_code: None,
            provider_session_id: Some("sess".into()),
            session_name: None,
            first_prompt: None,
            submit_prompt: None,
            title: None,
            unpersisted_session: None,
            remote: None,
            source: None,
            probe: None,
        }
    }

    #[test]
    fn snapshot_without_live_terminal_is_error() {
        let live = LiveBus::new();
        let runtime = HarnessRuntime::new(live.clone());
        let terminals = TerminalHub::new(live);
        let next = runtime.snapshot(&session("attention"), &terminals);
        assert_eq!(next.state, "error");
        assert_eq!(next.probe.as_deref(), Some("unconfirmed"));
    }

    #[test]
    fn snapshot_drops_missing_codex_rollout() {
        let live = LiveBus::new();
        let runtime = HarnessRuntime::new(live.clone());
        let terminals = TerminalHub::new(live);
        let mut current = session("exited");
        current.provider_session_id = Some("00000000-0000-0000-0000-000000000000".into());
        let next = runtime.snapshot(&current, &terminals);
        assert_eq!(next.provider_session_id, None);
        assert_eq!(next.unpersisted_session, Some(true));
    }
}
