use super::label_text::{json_text, SessionLabel};
use super::session_find::{find_all, find_first, safe_name_id};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::winproc::NoWindow;

struct TitleCache {
    path: PathBuf,
    stamp: String,
    titles: HashMap<String, String>,
    days: HashMap<String, String>,
}

static TITLES: Mutex<Option<TitleCache>> = Mutex::new(None);

fn codex_home() -> PathBuf {
    if let Ok(path) = std::env::var("CODEX_HOME") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".codex")
}

fn load_titles() {
    let path = codex_home().join("session_index.jsonl");
    let stamp = std::fs::metadata(&path)
        .ok()
        .and_then(|info| info.modified().ok())
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| format!("{}:{}", d.as_millis(), std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)))
        .unwrap_or_default();
    let mut cache = TITLES.lock().unwrap_or_else(|error| error.into_inner());
    if cache.as_ref().is_some_and(|c| c.path == path && c.stamp == stamp) {
        return;
    }
    let (titles, days) = std::fs::read_to_string(&path).ok().map(|body| parse_index(&body)).unwrap_or_default();
    *cache = Some(TitleCache { path, stamp, titles, days });
}

fn parse_index(body: &str) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut titles = HashMap::new();
    let mut days = HashMap::new();
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(id) = entry.get("id").and_then(|v| v.as_str()) else { continue };
        if let Some(name) = entry.get("thread_name").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
            titles.insert(id.to_string(), name.to_string());
        }
        if let Some(day) = entry.get("updated_at").and_then(|v| v.as_str()).and_then(index_day) {
            days.insert(id.to_string(), day);
        }
    }
    (titles, days)
}

fn index_day(updated_at: &str) -> Option<String> {
    let day = updated_at.get(..10)?;
    if day.as_bytes().get(4) == Some(&b'-') && day.as_bytes().get(7) == Some(&b'-') {
        Some(day.to_string())
    } else {
        None
    }
}

pub fn codex_exit_session_id(output: &str) -> Option<String> {
    let stripped = regex::Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").unwrap().replace_all(output, "");
    regex::Regex::new(r"(?i)\x1b\]0;(?:\x07|\x1b\\)Session ID: ([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\r?\n\s*$")
        .unwrap()
        .captures(&stripped)
        .map(|caps| caps[1].to_string())
}

pub fn session_label(session_id: &str, need_first_prompt: bool) -> SessionLabel {
    SessionLabel {
        name: codex_session_title(session_id),
        first_prompt: if need_first_prompt { first_user_prompt(session_id) } else { None },
    }
}

pub fn codex_session_title(session_id: &str) -> Option<String> {
    load_titles();
    TITLES.lock().unwrap_or_else(|error| error.into_inner()).as_ref()?.titles.get(session_id).cloned()
}

fn first_user_prompt(session_id: &str) -> Option<String> {
    first_user_from_rollout(&std::fs::read_to_string(locate_rollout(session_id)?).ok()?)
}

fn locate_rollout(session_id: &str) -> Option<PathBuf> {
    if !safe_name_id(session_id) {
        return None;
    }
    if let Some(path) = sqlite_rollout_path(session_id).filter(|path| path.is_file()) {
        return Some(path);
    }
    load_titles();
    let home = codex_home();
    let day = TITLES.lock().unwrap_or_else(|error| error.into_inner()).as_ref().and_then(|cache| cache.days.get(session_id).cloned());
    if let Some(day) = day {
        let y = &day[..4];
        let m = &day[5..7];
        let d = &day[8..10];
        let suffix = format!("{session_id}.jsonl");
        for directory in ["sessions", "archived_sessions"] {
            let folder = home.join(directory).join(y).join(m).join(d);
            let Ok(entries) = std::fs::read_dir(folder) else { continue };
            if let Some(path) = entries.flatten().map(|entry| entry.path()).find(|path| {
                path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.ends_with(&suffix))
            }) {
                return Some(path);
            }
        }
    }
    find_first(
        &[home.join("sessions"), home.join("archived_sessions")],
        &format!("*{session_id}.jsonl"),
    )
}

fn sqlite_rollout_path(session_id: &str) -> Option<PathBuf> {
    let db = codex_home().join("state_5.sqlite");
    if !db.exists() {
        return None;
    }
    let output = std::process::Command::new("sqlite3")
        .args([
            "-readonly",
            db.to_str()?,
            &format!("select rollout_path from threads where id='{session_id}'"),
        ])
        .no_window()
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(PathBuf::from(path)) }
}

fn first_user_from_rollout(body: &str) -> Option<String> {
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if entry.get("type").and_then(|v| v.as_str()) != Some("response_item") {
            continue;
        }
        let Some(payload) = entry.get("payload") else { continue };
        if payload.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        if let Some(text) = json_text(payload.get("content").unwrap_or(payload)) {
            return Some(text);
        }
    }
    None
}

pub fn resolve_codex_session_prefix(prefix: &str) -> Option<String> {
    if !regex::Regex::new(r"(?i)^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{5}$").unwrap().is_match(prefix) {
        return None;
    }
    load_titles();
    let mut matches = HashSet::new();
    if let Some(cache) = TITLES.lock().unwrap_or_else(|error| error.into_inner()).as_ref() {
        matches.extend(cache.titles.keys().chain(cache.days.keys()).filter(|id| id.starts_with(prefix)).cloned());
    }
    if matches.len() == 1 {
        return matches.into_iter().next();
    }
    let home = codex_home();
    let uuid = regex::Regex::new(r"(?i)([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\.jsonl$").unwrap();
    for path in find_all(
        &[home.join("sessions"), home.join("archived_sessions")],
        &format!("*{prefix}*.jsonl"),
    ) {
        if let Some(caps) = path.file_name().and_then(|name| name.to_str()).and_then(|name| uuid.captures(name)) {
            matches.insert(caps[1].to_string());
        }
    }
    if matches.len() == 1 { matches.into_iter().next() } else { None }
}

pub fn codex_session_exists(id: &str) -> Option<bool> {
    if !regex::Regex::new(r"(?i)^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$").unwrap().is_match(id) {
        return None;
    }
    Some(locate_rollout(id).is_some())
}

#[derive(Debug, Clone, Default)]
pub struct CodexSessionDetails {
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub prompt: Option<String>,
    pub reply: Option<String>,
    pub turns: Vec<crate::models::ExternalTurn>,
}

fn extract_rollout_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        }
        serde_json::Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                } else if let Some(text) = extract_rollout_text(item) {
                    parts.push(text);
                }
            }
            if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
        }
        serde_json::Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            if let Some(content) = map.get("content") {
                return extract_rollout_text(content);
            }
            None
        }
        _ => None,
    }
}

pub fn codex_session_details(session_id: &str) -> Option<CodexSessionDetails> {
    let path = locate_rollout(session_id)?;
    let body = std::fs::read_to_string(path).ok()?;
    let mut cwd = None;
    let mut turns = Vec::new();
    let mut last_user = None;
    let mut last_assistant = None;

    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let entry_type = entry.get("type").and_then(|v| v.as_str());
        if entry_type == Some("session_meta") {
            if let Some(payload) = entry.get("payload") {
                if let Some(c) = payload.get("cwd").and_then(|v| v.as_str()) {
                    cwd = Some(c.to_string());
                }
            } else if let Some(c) = entry.get("cwd").and_then(|v| v.as_str()) {
                cwd = Some(c.to_string());
            }
            continue;
        }
        if entry_type == Some("response_item") {
            let Some(payload) = entry.get("payload") else { continue };
            let Some(role) = payload.get("role").and_then(|v| v.as_str()) else { continue };
            if role != "user" && role != "assistant" {
                continue;
            }
            let raw_content = payload.get("content").unwrap_or(payload);
            let text = extract_rollout_text(raw_content);
            let Some(text) = text else { continue };
            if role == "user" {
                if super::label_text::is_noise(&text) {
                    continue;
                }
                last_user = Some(text.clone());
                turns.push(crate::models::ExternalTurn {
                    role: "user".into(),
                    text,
                });
            } else if role == "assistant" {
                last_assistant = Some(text.clone());
                turns.push(crate::models::ExternalTurn {
                    role: "assistant".into(),
                    text,
                });
            }
        }
    }

    Some(CodexSessionDetails {
        title: codex_session_title(session_id),
        cwd,
        prompt: last_user,
        reply: last_assistant,
        turns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_codex_exit_footer() {
        let output = "\x1b]0;\x07Session ID: 12345678-1234-1234-1234-123456789abc\n";
        assert_eq!(codex_exit_session_id(output).as_deref(), Some("12345678-1234-1234-1234-123456789abc"));
    }

    #[test]
    fn index_keeps_day_without_title() {
        let (titles, days) = parse_index(r#"{"id":"abc","updated_at":"2026-07-30T07:55:40.566345Z"}"#);
        assert!(titles.is_empty());
        assert_eq!(days.get("abc").map(String::as_str), Some("2026-07-30"));
    }

    #[test]
    fn first_user_skips_environment() {
        let body = r#"
{"type":"session_meta"}
{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"<environment_context>\ncwd"}]}}
{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"嗷嗷"}]}}
"#;
        assert_eq!(first_user_from_rollout(body).as_deref(), Some("嗷嗷"));
    }
}
