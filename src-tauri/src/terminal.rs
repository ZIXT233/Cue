use crate::error::{AppError, AppResult};
use crate::live::LiveBus;
use crate::models::HarnessSession;
use crate::remote::PtyCommand;
use crate::transcript::{read_terminal_transcript, save_terminal_transcript, TerminalTranscript};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const MAX_BACKLOG: usize = 128 * 1024;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    /// `from` is the byte offset where `data` starts, `offset` where it ends.
    /// Clients use `from` to detect gaps in the stream and resync instead of
    /// rendering a hole as terminal garbage.
    Output { data: String, from: u64, offset: u64, #[serde(skip_serializing_if = "Option::is_none")] reset: Option<bool> },
    Exit { #[serde(rename = "exitCode")] exit_code: i32 },
    Closed,
}

/// Where a session's bytes come from.
///
/// Local sessions still need a pty: `cmd.exe`, PowerShell and `$SHELL` all check
/// `isatty()` before they will behave like an interactive shell. Remote sessions
/// do not. Their pty is created by the SSH server on the far side, so a second,
/// local pty adds nothing — and on Windows ConPTY is not even a transparent
/// pipe: it renders into a screen buffer and re-serializes the escape sequences
/// before they reach the front end.
pub enum Spawn {
    Local { executable: String, args: Vec<String>, env: HashMap<String, String> },
    Remote { host: String, command: String },
}

pub struct TerminalSnapshot {
    pub cwd: String,
    pub exited: bool,
    pub exit_code: Option<i32>,
    pub output: String,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyProbe {
    pub reader_alive: bool,
    pub chunks: u64,
    pub bytes_in: u64,
    pub decode_held: u64,
    pub events_emitted: u64,
    pub listeners: usize,
    pub send_fail: u64,
    pub backlog_bytes: usize,
    pub offset: u64,
    pub writes: u64,
    pub write_ok: u64,
    pub write_err: u64,
    pub last_write_ms: u64,
    pub last_write_bytes: usize,
    pub resize_count: u64,
    pub last_resize_cols: u16,
    pub last_resize_rows: u16,
    pub last_resize_ok: bool,
    pub ioctl_cols: Option<u16>,
    pub ioctl_rows: Option<u16>,
    pub on_output_panic: u64,
    /// "local" or "ssh" — makes it obvious which pipeline a session used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_focus: Option<String>,
}

/// The write/resize/kill side of a session, whichever transport is behind it.
///
/// Deliberately synchronous: `TerminalHub`'s write/resize/kill are called from
/// synchronous code paths, so the remote implementation forwards onto a queue
/// that the SSH task drains rather than making every caller async.
trait Channel: Send + Sync {
    fn write(&self, data: &[u8]) -> bool;
    fn resize(&self, cols: u16, rows: u16) -> bool;
    fn size(&self) -> Option<(u16, u16)>;
    fn kill(&self);
}

struct LocalChannel {
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>,
}

impl Channel for LocalChannel {
    fn write(&self, data: &[u8]) -> bool {
        lock(&self.writer).as_mut().is_some_and(|writer| writer.write_all(data).is_ok())
    }

    fn resize(&self, cols: u16, rows: u16) -> bool {
        lock(&self.master)
            .as_mut()
            .is_some_and(|master| master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).is_ok())
    }

    fn size(&self) -> Option<(u16, u16)> {
        lock(&self.master).as_ref().and_then(|master| master.get_size().ok()).map(|size| (size.cols, size.rows))
    }

    fn kill(&self) {
        if let Some(killer) = lock(&self.killer).as_mut() {
            let _ = killer.kill();
        }
    }
}

struct RemoteChannel {
    commands: mpsc::UnboundedSender<PtyCommand>,
    size: Mutex<(u16, u16)>,
    closed: AtomicBool,
}

impl Channel for RemoteChannel {
    fn write(&self, data: &[u8]) -> bool {
        if self.closed.load(Ordering::Relaxed) {
            return false;
        }
        self.commands.send(PtyCommand::Data(data.to_vec())).is_ok()
    }

    fn resize(&self, cols: u16, rows: u16) -> bool {
        if self.commands.send(PtyCommand::Resize(cols, rows)).is_err() {
            return false;
        }
        *lock(&self.size) = (cols, rows);
        true
    }

    fn size(&self) -> Option<(u16, u16)> {
        Some(*lock(&self.size))
    }

    fn kill(&self) {
        self.closed.store(true, Ordering::Relaxed);
        let _ = self.commands.send(PtyCommand::Close);
    }
}

struct Record {
    cwd: String,
    backlog: String,
    offset: u64,
    /// Bytes of an incomplete UTF-8 sequence held over from the previous chunk.
    pending: Vec<u8>,
    exited: bool,
    exit_code: Option<i32>,
    persistent: bool,
    channel: Arc<dyn Channel>,
    last_listener: Instant,
    listeners: usize,
    on_output: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    listeners_tx: Vec<mpsc::UnboundedSender<TerminalEvent>>,
    dirty: bool,
    probe: Arc<Mutex<PtyProbe>>,
}

#[derive(Clone)]
pub struct TerminalHub {
    inner: Arc<Mutex<HashMap<String, Record>>>,
    live: LiveBus,
}

impl TerminalHub {
    pub fn new(live: LiveBus) -> Self {
        let inner = Arc::new(Mutex::new(HashMap::new()));
        let hub = Self { inner: inner.clone(), live };
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            persist_dirty(&inner);
        });
        hub
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Record>> {
        lock(&self.inner)
    }

    pub fn create(
        &self,
        cwd: String,
        cols: u16,
        rows: u16,
        id: Option<String>,
        spawn: Spawn,
        persistent: bool,
        on_output: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    ) -> AppResult<String> {
        let cwd = if cwd.is_empty() {
            dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).to_string_lossy().into_owned()
        } else {
            cwd
        };
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        {
            let map = self.lock();
            if let Some(existing) = map.get(&id) {
                if existing.cwd != cwd {
                    return Err(AppError::msg("Terminal belongs to a different workspace"));
                }
                return Ok(id);
            }
        }
        let cols = dimension(cols, 80);
        let rows = dimension(rows, 24);
        let transport = match &spawn {
            Spawn::Local { .. } => "local",
            Spawn::Remote { .. } => "ssh",
        };
        crate::debuglog::log(&format!(
            "terminal: hub create id={id} transport={transport} cwd={cwd:?}{}",
            match &spawn {
                Spawn::Local { executable, .. } => format!(" executable={executable:?}"),
                Spawn::Remote { host, command } => format!(" host={host:?} cmd={}", crate::debuglog::clip(command, 160)),
            }
        ));
        let probe = Arc::new(Mutex::new(PtyProbe { reader_alive: true, transport: Some(transport.into()), ..PtyProbe::default() }));
        match spawn {
            Spawn::Local { executable, args, env } => self.spawn_local(cwd, cols, rows, id.clone(), executable, args, env, persistent, on_output, probe)?,
            Spawn::Remote { host, command } => self.spawn_remote(cwd, cols, rows, id.clone(), host, command, persistent, on_output, probe),
        }
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_local(
        &self,
        cwd: String,
        cols: u16,
        rows: u16,
        id: String,
        executable: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        persistent: bool,
        on_output: Option<Arc<dyn Fn(&str) + Send + Sync>>,
        probe: Arc<Mutex<PtyProbe>>,
    ) -> AppResult<()> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| AppError::msg(e.to_string()))?;
        let mut cmd = CommandBuilder::new(&executable);
        cmd.args(&args);
        cmd.cwd(&cwd);
        for (key, value) in env {
            cmd.env(key, value);
        }
        // Size must come from the PTY ioctl. COLUMNS/LINES inherited from the
        // Tauri/dev terminal (or a login-shell dump) make Ink/OpenCode/Cursor
        // draw the input row against the parent size, not this session.
        cmd.env_remove("COLUMNS");
        cmd.env_remove("LINES");
        cmd.env_remove("NO_COLOR");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        if cmd.get_env("LANG").is_none() && cmd.get_env("LC_ALL").is_none() && cmd.get_env("LC_CTYPE").is_none() {
            cmd.env("LANG", "C.UTF-8");
        }
        let child = pair.slave.spawn_command(cmd).map_err(|e| AppError::msg(e.to_string()))?;
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(|e| AppError::msg(e.to_string()))?;
        let writer = pair.master.take_writer().map_err(|e| AppError::msg(e.to_string()))?;
        let channel: Arc<dyn Channel> = Arc::new(LocalChannel {
            writer: Mutex::new(Some(writer)),
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(Some(killer)),
        });
        self.register(id.clone(), cwd, persistent, on_output, channel, probe.clone());

        let inner = self.inner.clone();
        let live = self.live.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => deliver(&inner, &id, &probe, &buffer[..n]),
                    Err(error) => {
                        #[cfg(debug_assertions)]
                        if crate::dev_tools::probes_enabled() {
                            lock(&probe).last_error = Some(error.to_string());
                        }
                        #[cfg(not(debug_assertions))]
                        let _ = error;
                        break;
                    }
                }
            }
            let mut child = child;
            let code = child.wait().ok().map(|s| s.exit_code() as i32).unwrap_or(0);
            finalize(&inner, &id, code, &live, &probe);
        });
        Ok(())
    }

    /// A remote session: one SSH channel carrying the pty, driven by a task that
    /// also drains the write/resize queue.
    #[allow(clippy::too_many_arguments)]
    fn spawn_remote(
        &self,
        cwd: String,
        cols: u16,
        rows: u16,
        id: String,
        host: String,
        command: String,
        persistent: bool,
        on_output: Option<Arc<dyn Fn(&str) + Send + Sync>>,
        probe: Arc<Mutex<PtyProbe>>,
    ) {
        let (commands, queue) = mpsc::unbounded_channel();
        let channel: Arc<dyn Channel> = Arc::new(RemoteChannel { commands, size: Mutex::new((cols, rows)), closed: AtomicBool::new(false) });
        self.register(id.clone(), cwd, persistent, on_output, channel, probe.clone());

        let inner = self.inner.clone();
        let live = self.live.clone();
        let sink_inner = inner.clone();
        let sink_id = id.clone();
        let sink_probe = probe.clone();
        tokio::spawn(async move {
            crate::debuglog::log(&format!("terminal: remote spawn id={id} host={host:?} {cols}x{rows} cmd={}", crate::debuglog::clip(&command, 200)));
            let result = crate::remote::run_pty(&host, &command, cols, rows, move |data: Vec<u8>| {
                deliver(&sink_inner, &sink_id, &sink_probe, &data);
            }, queue)
            .await;
            let code = match result {
                Ok(code) => {
                    crate::debuglog::log(&format!("terminal: remote spawn id={id} ended with exit code {code}"));
                    code
                }
                Err(error) => {
                    crate::debuglog::log_error(&format!("terminal: remote spawn id={id} FAILED"), &error);
                    // Report the failure in the pane, where `ssh`'s own stderr
                    // used to land, instead of silently ending the session.
                    let text = format!("\r\n\x1b[31m{error}\x1b[0m\r\n");
                    deliver(&inner, &id, &probe, text.as_bytes());
                    1
                }
            };
            finalize(&inner, &id, code, &live, &probe);
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn register(
        &self,
        id: String,
        cwd: String,
        persistent: bool,
        on_output: Option<Arc<dyn Fn(&str) + Send + Sync>>,
        channel: Arc<dyn Channel>,
        probe: Arc<Mutex<PtyProbe>>,
    ) {
        let mut map = self.lock();
        map.insert(id, Record {
            cwd,
            backlog: String::new(),
            offset: 0,
            pending: Vec::new(),
            exited: false,
            exit_code: None,
            persistent,
            channel,
            last_listener: Instant::now(),
            listeners: 0,
            on_output,
            listeners_tx: Vec::new(),
            dirty: false,
            probe,
        });
    }

    /// A side terminal on the local machine: the user's own shell.
    pub fn create_shell(&self, cwd: String, cols: u16, rows: u16, id: Option<String>) -> AppResult<String> {
        let (executable, args) = if cfg!(windows) {
            (std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into()), Vec::new())
        } else {
            (std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()), vec!["-l".into()])
        };
        self.create(cwd, cols, rows, id, Spawn::Local { executable, args, env: HashMap::new() }, false, None)
    }

    /// A side terminal on an SSH host: the remote user's login shell, started in
    /// the workspace's remote directory.
    ///
    /// `cwd` is a path on `host`, not here — it is what the shell `cd`s into, and
    /// the record's `cwd` only has to stay stable across re-attaches of the same
    /// terminal id.
    pub fn create_remote_shell(&self, cwd: String, cols: u16, rows: u16, id: Option<String>, host: String) -> AppResult<String> {
        let command = crate::ssh::remote_login_shell(&cwd);
        self.create(cwd, cols, rows, id, Spawn::Remote { host, command }, false, None)
    }

    pub fn snapshot(&self, id: &str) -> Option<TerminalSnapshot> {
        self.lock().get(id).map(|r| TerminalSnapshot {
            cwd: r.cwd.clone(),
            exited: r.exited,
            exit_code: r.exit_code,
            output: r.backlog.clone(),
        })
    }

    pub fn cwd(&self, id: &str) -> Option<String> {
        self.lock()
            .get(id)
            .map(|r| r.cwd.clone())
            .or_else(|| read_terminal_transcript(id).map(|saved| saved.cwd))
    }

    pub fn write(&self, id: &str, data: &str) -> bool {
        let (channel, probe) = {
            let map = self.lock();
            let Some(record) = map.get(id) else { return false };
            if record.exited { return false; }
            (record.channel.clone(), record.probe.clone())
        };
        #[cfg(debug_assertions)]
        let started = Instant::now();
        let ok = channel.write(data.as_bytes());
        #[cfg(debug_assertions)]
        if crate::dev_tools::probes_enabled() {
            let elapsed = started.elapsed().as_millis() as u64;
            let mut probe = lock(&probe);
            probe.writes += 1;
            probe.last_write_ms = elapsed;
            probe.last_write_bytes = data.len();
            if data == "\x1b[I" {
                probe.last_focus = Some("focused".into());
            } else if data == "\x1b[O" {
                probe.last_focus = Some("unfocused".into());
            }
            if ok { probe.write_ok += 1; } else { probe.write_err += 1; probe.last_error = Some("pty write failed".into()); }
        }
        #[cfg(not(debug_assertions))]
        let _ = probe;
        ok
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> bool {
        let (channel, probe) = {
            let map = self.lock();
            let Some(record) = map.get(id) else { return false };
            (record.channel.clone(), record.probe.clone())
        };
        let ok = channel.resize(cols, rows);
        #[cfg(debug_assertions)]
        if crate::dev_tools::probes_enabled() {
            let size = channel.size();
            let mut probe = lock(&probe);
            probe.resize_count += 1;
            probe.last_resize_cols = cols;
            probe.last_resize_rows = rows;
            probe.last_resize_ok = ok;
            if let Some((cols, rows)) = size {
                probe.ioctl_cols = Some(cols);
                probe.ioctl_rows = Some(rows);
            }
            if !ok {
                probe.last_error = Some("pty resize failed".into());
            }
        }
        #[cfg(not(debug_assertions))]
        let _ = probe;
        ok
    }

    pub fn probe(&self, id: &str) -> Option<PtyProbe> {
        let map = self.lock();
        let record = map.get(id)?;
        let mut probe = lock(&record.probe).clone();
        probe.backlog_bytes = record.backlog.len();
        probe.offset = record.offset;
        probe.listeners = record.listeners_tx.len();
        if let Some((cols, rows)) = record.channel.size() {
            probe.ioctl_cols = Some(cols);
            probe.ioctl_rows = Some(rows);
        }
        Some(probe)
    }

    pub fn kill(&self, id: &str) {
        let mut map = self.lock();
        if let Some(mut record) = map.remove(id) {
            let saved = record.persistent.then(|| TerminalTranscript {
                cwd: record.cwd.clone(),
                output: record.backlog.clone(),
                exit_code: record.exit_code,
            });
            if !record.exited {
                record.channel.kill();
            }
            emit(&mut record, TerminalEvent::Closed);
            // Dropping the record drops the channel: a local pty loses its
            // master handle, a remote queue loses its sender and the SSH task
            // tears the channel down.
            drop(map);
            if let Some(saved) = saved {
                save_terminal_transcript(id, &saved);
            }
        }
    }

    pub fn stop(&self, id: &str) {
        let channel = {
            let map = self.lock();
            let Some(record) = map.get(id) else { return };
            if record.exited { return; }
            record.channel.clone()
        };
        channel.kill();
        let inner = self.inner.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            let map = lock(&inner);
            if let Some(record) = map.get(&id) {
                if !record.exited {
                    record.channel.kill();
                }
            }
        });
    }

    pub fn shutdown(&self) {
        let ids: Vec<String> = self.lock().keys().cloned().collect();
        for id in ids {
            self.kill(&id);
        }
    }

    pub fn subscribe(&self, id: &str, after: Option<u64>) -> Option<(TerminalEvent, mpsc::UnboundedReceiver<TerminalEvent>, bool, Option<i32>)> {
        let mut map = self.lock();
        if let Some(record) = map.get_mut(id) {
            let (tx, rx) = mpsc::unbounded_channel();
            record.listeners_tx.push(tx);
            record.listeners = record.listeners_tx.len();
            record.last_listener = Instant::now();
            let start = record.offset.saturating_sub(record.backlog.len() as u64);
            let reset = after.is_none() || after.is_some_and(|cursor| cursor < start || cursor > record.offset);
            let (data, from) = if reset {
                (record.backlog.clone(), start)
            } else {
                let skip = after.unwrap_or(start).saturating_sub(start) as usize;
                let mut index = skip.min(record.backlog.len());
                while index < record.backlog.len() && !record.backlog.is_char_boundary(index) {
                    index += 1;
                }
                (record.backlog[index..].to_string(), start + index as u64)
            };
            return Some((
                TerminalEvent::Output { data, from, offset: record.offset, reset: Some(reset) },
                rx,
                record.exited,
                record.exit_code,
            ));
        }
        drop(map);
        let saved = read_terminal_transcript(id)?;
        let (_tx, rx) = mpsc::unbounded_channel();
        Some((
            TerminalEvent::Output {
                data: saved.output.clone(),
                from: 0,
                offset: saved.output.len() as u64,
                reset: Some(true),
            },
            rx,
            true,
            saved.exit_code,
        ))
    }

    pub fn release_listener(&self, id: &str) {
        let mut map = self.lock();
        if let Some(record) = map.get_mut(id) {
            record.listeners_tx.retain(|tx| !tx.is_closed());
            record.listeners = record.listeners_tx.len();
            record.last_listener = Instant::now();
        }
    }

    pub fn harness_overlay(&self, _id: &str) -> Option<HarnessSession> {
        None
    }
}

/// Hand one chunk of raw transport bytes to the terminal it belongs to.
///
/// Both transports funnel through here so the local pty thread and the SSH task
/// cannot drift apart on backlog trimming, offset accounting or the dev probes.
fn deliver(inner: &Mutex<HashMap<String, Record>>, id: &str, probe: &Arc<Mutex<PtyProbe>>, incoming: &[u8]) {
    #[cfg(not(debug_assertions))]
    let _ = probe;
    let (data, callback) = {
        let mut map = lock(inner);
        let Some(record) = map.get_mut(id) else { return };
        let data = decode_chunk(&mut record.pending, incoming);
        #[cfg(debug_assertions)]
        if crate::dev_tools::probes_enabled() {
            let mut probe = lock(probe);
            probe.chunks += 1;
            probe.bytes_in += incoming.len() as u64;
            if data.is_empty() {
                probe.decode_held += 1;
            }
        }
        if data.is_empty() {
            return;
        }
        record.backlog.push_str(&data);
        let from = record.offset;
        record.offset += data.len() as u64;
        trim_backlog(&mut record.backlog);
        if record.persistent {
            record.dirty = true;
        }
        #[cfg(debug_assertions)]
        let before = record.listeners_tx.len();
        emit(record, TerminalEvent::Output { data: data.clone(), from, offset: record.offset, reset: None });
        #[cfg(debug_assertions)]
        if crate::dev_tools::probes_enabled() {
            let mut probe = lock(probe);
            probe.events_emitted += 1;
            probe.backlog_bytes = record.backlog.len();
            probe.offset = record.offset;
            probe.listeners = record.listeners_tx.len();
            if record.listeners_tx.len() < before {
                probe.send_fail += (before - record.listeners_tx.len()) as u64;
            }
        }
        (data, record.on_output.clone())
    };
    if let Some(callback) = callback {
        if catch_unwind(AssertUnwindSafe(|| callback(&data))).is_err() {
            #[cfg(debug_assertions)]
            if crate::dev_tools::probes_enabled() {
                let mut probe = lock(probe);
                probe.on_output_panic += 1;
                probe.last_error = Some("on_output panicked".into());
            }
        }
    }
}

fn finalize(inner: &Mutex<HashMap<String, Record>>, id: &str, code: i32, live: &LiveBus, probe: &Arc<Mutex<PtyProbe>>) {
    lock(probe).reader_alive = false;
    let saved = {
        let mut map = lock(inner);
        map.get_mut(id).map(|record| {
            record.exited = true;
            record.exit_code = Some(code);
            record.dirty = false;
            let saved = TerminalTranscript {
                cwd: record.cwd.clone(),
                output: record.backlog.clone(),
                exit_code: record.exit_code,
            };
            emit(record, TerminalEvent::Exit { exit_code: code });
            (record.persistent, saved)
        })
    };
    if let Some((true, saved)) = saved {
        save_terminal_transcript(id, &saved);
    }
    live.notify("terminal");
}

fn emit(record: &mut Record, event: TerminalEvent) {
    record.listeners_tx.retain(|tx| tx.send(event.clone()).is_ok());
    record.listeners = record.listeners_tx.len();
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn decode_chunk(pending: &mut Vec<u8>, incoming: &[u8]) -> String {
    pending.extend_from_slice(incoming);
    match std::str::from_utf8(pending) {
        Ok(text) => {
            let out = text.to_string();
            pending.clear();
            out
        }
        Err(error) => {
            let valid = error.valid_up_to();
            let out = std::str::from_utf8(&pending[..valid]).unwrap_or("").to_string();
            if let Some(invalid) = error.error_len() {
                pending.drain(..valid + invalid);
            } else {
                pending.drain(..valid);
            }
            out
        }
    }
}

fn dimension(value: u16, fallback: u16) -> u16 {
    let n = if value < 2 { fallback } else { value };
    n.clamp(2, 1000)
}

fn persist_dirty(inner: &Mutex<HashMap<String, Record>>) {
    let snapshots: Vec<(String, TerminalTranscript)> = {
        let mut map = lock(inner);
        map.iter_mut()
            .filter(|(_, record)| record.persistent && record.dirty)
            .map(|(id, record)| {
                record.dirty = false;
                (id.clone(), TerminalTranscript {
                    cwd: record.cwd.clone(),
                    output: record.backlog.clone(),
                    exit_code: record.exit_code,
                })
            })
            .collect()
    };
    for (id, saved) in snapshots {
        save_terminal_transcript(&id, &saved);
    }
}

fn trim_backlog(backlog: &mut String) {
    if backlog.len() <= MAX_BACKLOG {
        return;
    }
    let mut extra = backlog.len() - MAX_BACKLOG;
    while extra < backlog.len() && !backlog.is_char_boundary(extra) {
        extra += 1;
    }
    backlog.drain(..extra);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::LiveBus;
    use std::sync::Mutex;

    static ENV: Mutex<()> = Mutex::new(());

    #[test]
    fn cwd_and_subscribe_fall_back_to_transcript() {
        let _guard = ENV.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("cue-term-{}", uuid::Uuid::new_v4().simple()));
        std::env::set_var("CUE_DATA_DIR", &dir);
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        save_terminal_transcript(id, &TerminalTranscript {
            cwd: "/work".into(),
            output: "kept\n".into(),
            exit_code: Some(0),
        });
        let hub = TerminalHub::new(LiveBus::new());
        assert_eq!(hub.cwd(id).as_deref(), Some("/work"));
        assert!(hub.snapshot(id).is_none());
        let (output, _, exited, code) = hub.subscribe(id, None).expect("transcript subscribe");
        assert!(exited);
        assert_eq!(code, Some(0));
        match output {
            TerminalEvent::Output { data, reset, .. } => {
                assert_eq!(data, "kept\n");
                assert_eq!(reset, Some(true));
            }
            _ => panic!("expected replayed output"),
        }
        let _ = std::fs::remove_dir_all(dir);
        std::env::remove_var("CUE_DATA_DIR");
    }

    #[test]
    fn decode_chunk_holds_incomplete_utf8() {
        let mut pending = Vec::new();
        let bytes = "你好".as_bytes();
        assert!(decode_chunk(&mut pending, &bytes[..1]).is_empty());
        assert_eq!(decode_chunk(&mut pending, &bytes[1..]), "你好");
        assert!(pending.is_empty());
    }

    /// The two transports must agree on how a chunk reaches the front end, so
    /// pin the shared ingest path rather than either transport's plumbing.
    #[test]
    fn deliver_merges_split_utf8_across_chunks() {
        let hub = TerminalHub::new(LiveBus::new());
        let id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let probe = Arc::new(Mutex::new(PtyProbe { reader_alive: true, ..PtyProbe::default() }));
        hub.register(id.to_string(), "/work".into(), false, None, Arc::new(NoopChannel), probe.clone());
        let bytes = "你好".as_bytes();
        deliver(&hub.inner, id, &probe, &bytes[..1]);
        assert_eq!(hub.snapshot(id).map(|s| s.output), Some(String::new()));
        deliver(&hub.inner, id, &probe, &bytes[1..]);
        assert_eq!(hub.snapshot(id).map(|s| s.output), Some("你好".into()));
    }

    struct NoopChannel;

    impl Channel for NoopChannel {
        fn write(&self, _data: &[u8]) -> bool { false }
        fn resize(&self, _cols: u16, _rows: u16) -> bool { false }
        fn size(&self) -> Option<(u16, u16)> { None }
        fn kill(&self) {}
    }

    /// The whole translation between the hub's synchronous write/resize/kill and
    /// the SSH task's queue. A side terminal on a remote host goes through this
    /// and nothing else.
    #[test]
    fn a_remote_channel_forwards_writes_sizes_and_the_close() {
        let (commands, mut queue) = mpsc::unbounded_channel();
        let channel = RemoteChannel { commands, size: Mutex::new((80, 24)), closed: AtomicBool::new(false) };

        assert_eq!(channel.size(), Some((80, 24)));
        assert!(channel.write(b"ls\r"));
        assert!(channel.resize(120, 40));
        assert_eq!(channel.size(), Some((120, 40)));
        assert!(channel.write("\u{4f60}\u{597d}".as_bytes()));

        // A killed channel stops accepting input — the pane is gone — but still
        // tells the SSH task to close, so the remote shell cannot outlive it.
        channel.kill();
        assert!(!channel.write(b"stale"));

        let mut seen = Vec::new();
        while let Ok(command) = queue.try_recv() {
            seen.push(command);
        }
        assert_eq!(seen.len(), 4);
        assert!(matches!(seen[0], PtyCommand::Data(ref data) if data == b"ls\r"));
        assert!(matches!(seen[1], PtyCommand::Resize(120, 40)));
        assert!(matches!(seen[2], PtyCommand::Data(ref data) if data == "\u{4f60}\u{597d}".as_bytes()));
        assert!(matches!(seen[3], PtyCommand::Close));
    }
}
