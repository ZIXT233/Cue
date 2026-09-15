//! Lightweight file logger for debugging the SSH chain.
//! Writes timestamped lines to %TEMP%\cue-debug.log (or /tmp/cue-debug.log).
//! Enabled when CUE_DEBUG=1 (or always on debug builds).

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOCK: Mutex<()> = Mutex::new(());

pub fn enabled() -> bool {
    // Temporarily always on: the SSH chain is being diagnosed, and the user
    // runs a build where the old debug-only switch left the log empty. Set
    // CUE_DEBUG=0 to silence it again.
    std::env::var("CUE_DEBUG").map(|v| v != "0").unwrap_or(true)
}

pub fn log_path() -> std::path::PathBuf {
    std::env::temp_dir().join("cue-debug.log")
}

/// UTC timestamp formatted from unix seconds (no chrono dependency).
fn timestamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

pub fn log(message: &str) {
    if !enabled() {
        return;
    }
    let line = format!("[{}] {}", timestamp(), message);
    eprintln!("{line}");
    let _guard = LOCK.lock();
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(file, "{line}");
    }
}

/// Log an AppError with its code/prompt shape.
pub fn log_error(context: &str, error: &crate::error::AppError) {
    match error {
        crate::error::AppError::Machine { code, prompt, detail } => match (prompt, detail) {
            (Some(p), Some(d)) => log(&format!("{context} -> Machine code={code} prompt={p:?} detail={d:?}")),
            (Some(p), None) => log(&format!("{context} -> Machine code={code} prompt={p:?}")),
            (None, Some(d)) => log(&format!("{context} -> Machine code={code} detail={d:?}")),
            (None, None) => log(&format!("{context} -> Machine code={code}")),
        },
        crate::error::AppError::Message(m) => log(&format!("{context} -> Message: {m}")),
    }
}

/// Truncate long payloads for logging.
pub fn clip(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}…(len={})", &value[..max], value.len())
    }
}
