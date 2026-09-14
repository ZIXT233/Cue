use crate::error::{AppError, AppResult};
use crate::live::LiveBus;
use crate::models::HarnessSession;
use crate::transcript::{read_terminal_transcript, save_terminal_transcript, TerminalTranscript};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const MAX_BACKLOG: usize = 128 * 1024;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    Output { data: String, offset: u64, #[serde(skip_serializing_if = "Option::is_none")] reset: Option<bool> },
    Exit { #[serde(rename = "exitCode")] exit_code: i32 },
    Closed,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_focus: Option<String>,
}

struct Record {
    cwd: String,
    backlog: String,
    offset: u64,
    exited: bool,
    exit_code: Option<i32>,
    persistent: bool,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    last_listener: Instant,
    listeners: usize,
    on_output: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    listeners_tx: Vec<mpsc::UnboundedSender<TerminalEvent>>,
    dirty: bool,
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,
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
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn create(
        &self,
        cwd: String,
        cols: u16,
        rows: u16,
        id: Option<String>,
        executable: String,
        args: Vec<String>,
        env: HashMap<String, String>,
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
        let probe = Arc::new(Mutex::new(PtyProbe { reader_alive: true, ..PtyProbe::default() }));
        {
            let mut map = self.lock();
            map.insert(id.clone(), Record {
                cwd,
                backlog: String::new(),
                offset: 0,
                exited: false,
                exit_code: None,
                persistent,
                writer: Arc::new(Mutex::new(Some(writer))),
                master: Arc::new(Mutex::new(Some(pair.master))),
                last_listener: Instant::now(),
                listeners: 0,
                on_output,
                listeners_tx: Vec::new(),
                dirty: false,
                killer: Some(killer),
                probe: probe.clone(),
            });
        }
        let inner = self.inner.clone();
        let live = self.live.clone();
        let id_out = id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut pending = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = decode_chunk(&mut pending, &buf[..n]);
                        #[cfg(debug_assertions)]
                        if crate::dev_tools::probes_enabled() {
                            let mut probe = probe.lock().unwrap_or_else(|error| error.into_inner());
                            probe.chunks += 1;
                            probe.bytes_in += n as u64;
                            if data.is_empty() {
                                probe.decode_held += 1;
                            }
                        }
                        if data.is_empty() {
                            continue;
                        }
                        let callback = {
                            let mut map = inner.lock().unwrap_or_else(|error| error.into_inner());
                            map.get_mut(&id_out).map(|record| {
                                record.backlog.push_str(&data);
                                record.offset += data.len() as u64;
                                trim_backlog(&mut record.backlog);
                                if record.persistent {
                                    record.dirty = true;
                                }
                                #[cfg(debug_assertions)]
                                let before = record.listeners_tx.len();
                                emit(record, TerminalEvent::Output { data: data.clone(), offset: record.offset, reset: None });
                                #[cfg(debug_assertions)]
                                if crate::dev_tools::probes_enabled() {
                                    let mut probe = record.probe.lock().unwrap_or_else(|error| error.into_inner());
                                    probe.events_emitted += 1;
                                    probe.backlog_bytes = record.backlog.len();
                                    probe.offset = record.offset;
                                    probe.listeners = record.listeners_tx.len();
                                    if record.listeners_tx.len() < before {
                                        probe.send_fail += (before - record.listeners_tx.len()) as u64;
                                    }
                                }
                                record.on_output.clone()
                            })
                        };
                        if let Some(Some(cb)) = callback {
                            if catch_unwind(AssertUnwindSafe(|| cb(&data))).is_err() {
                                #[cfg(debug_assertions)]
                                if crate::dev_tools::probes_enabled() {
                                    let mut probe = probe.lock().unwrap_or_else(|error| error.into_inner());
                                    probe.on_output_panic += 1;
                                    probe.last_error = Some("on_output panicked".into());
                                }
                            }
                        }
                    }
                    Err(error) => {
                        #[cfg(debug_assertions)]
                        if crate::dev_tools::probes_enabled() {
                            let mut probe = probe.lock().unwrap_or_else(|e| e.into_inner());
                            probe.last_error = Some(error.to_string());
                        }
                        #[cfg(not(debug_assertions))]
                        let _ = error;
                        break;
                    }
                }
            }
            probe.lock().unwrap_or_else(|error| error.into_inner()).reader_alive = false;
            let mut child = child;
            let code = child.wait().ok().map(|s| s.exit_code() as i32).unwrap_or(0);
            let saved = {
                let mut map = inner.lock().unwrap_or_else(|error| error.into_inner());
                map.get_mut(&id_out).map(|record| {
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
                save_terminal_transcript(&id_out, &saved);
            }
            live.notify("terminal");
        });
        Ok(id)
    }

    pub fn create_shell(&self, cwd: String, cols: u16, rows: u16, id: Option<String>) -> AppResult<String> {
        let (executable, args) = if cfg!(windows) {
            (std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into()), Vec::new())
        } else {
            (std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()), vec!["-l".into()])
        };
        self.create(cwd, cols, rows, id, executable, args, HashMap::new(), false, None)
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
        let (writer, probe) = {
            let map = self.lock();
            let Some(record) = map.get(id) else { return false };
            if record.exited { return false; }
            (record.writer.clone(), record.probe.clone())
        };
        #[cfg(debug_assertions)]
        let started = Instant::now();
        let mut guard = writer.lock().unwrap_or_else(|error| error.into_inner());
        let ok = guard.as_mut().is_some_and(|writer| writer.write_all(data.as_bytes()).is_ok());
        #[cfg(debug_assertions)]
        if crate::dev_tools::probes_enabled() {
            let elapsed = started.elapsed().as_millis() as u64;
            let mut probe = probe.lock().unwrap_or_else(|error| error.into_inner());
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
        let (master, probe) = {
            let map = self.lock();
            let Some(record) = map.get(id) else { return false };
            (record.master.clone(), record.probe.clone())
        };
        let mut guard = master.lock().unwrap_or_else(|error| error.into_inner());
        let ok = guard.as_mut().is_some_and(|master| master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).is_ok());
        #[cfg(debug_assertions)]
        if crate::dev_tools::probes_enabled() {
            let ioctl = guard.as_ref().and_then(|master| master.get_size().ok());
            let mut probe = probe.lock().unwrap_or_else(|error| error.into_inner());
            probe.resize_count += 1;
            probe.last_resize_cols = cols;
            probe.last_resize_rows = rows;
            probe.last_resize_ok = ok;
            if let Some(size) = ioctl {
                probe.ioctl_cols = Some(size.cols);
                probe.ioctl_rows = Some(size.rows);
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
        let mut probe = record.probe.lock().unwrap_or_else(|error| error.into_inner()).clone();
        probe.backlog_bytes = record.backlog.len();
        probe.offset = record.offset;
        probe.listeners = record.listeners_tx.len();
        if let Ok(master) = record.master.lock() {
            if let Some(size) = master.as_ref().and_then(|m| m.get_size().ok()) {
                probe.ioctl_cols = Some(size.cols);
                probe.ioctl_rows = Some(size.rows);
            }
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
            if let Some(mut killer) = record.killer.take() {
                if !record.exited {
                    let _ = killer.kill();
                }
            }
            emit(&mut record, TerminalEvent::Closed);
            if let Ok(mut writer) = record.writer.lock() {
                *writer = None;
            }
            if let Ok(mut master) = record.master.lock() {
                *master = None;
            }
            drop(map);
            if let Some(saved) = saved {
                save_terminal_transcript(id, &saved);
            }
        }
    }

    pub fn stop(&self, id: &str) {
        let mut map = self.lock();
        let Some(record) = map.get_mut(id) else { return };
        if record.exited { return; }
        if let Some(killer) = record.killer.as_mut() {
            let _ = killer.kill();
        }
        let inner = self.inner.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            let mut map = inner.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(record) = map.get_mut(&id) {
                if !record.exited {
                    if let Some(killer) = record.killer.as_mut() {
                        let _ = killer.kill();
                    }
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
            let data = if reset {
                record.backlog.clone()
            } else {
                let skip = after.unwrap_or(start).saturating_sub(start) as usize;
                let mut from = skip.min(record.backlog.len());
                while from < record.backlog.len() && !record.backlog.is_char_boundary(from) {
                    from += 1;
                }
                record.backlog[from..].to_string()
            };
            return Some((
                TerminalEvent::Output { data, offset: record.offset, reset: Some(reset) },
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

fn emit(record: &mut Record, event: TerminalEvent) {
    record.listeners_tx.retain(|tx| tx.send(event.clone()).is_ok());
    record.listeners = record.listeners_tx.len();
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
        let mut map = inner.lock().unwrap_or_else(|error| error.into_inner());
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
}
