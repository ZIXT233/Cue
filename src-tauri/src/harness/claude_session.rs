use super::label_text::{clip, json_text, SessionLabel};
use super::session_find::{find_dir, find_first, safe_name_id};
use std::path::PathBuf;

fn claude_home() -> PathBuf {
    if let Ok(path) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".claude")
}

pub(crate) fn name_from_sidecar(body: &str) -> Option<String> {
    let entry = serde_json::from_str::<serde_json::Value>(body).ok()?;
    entry
        .get("customTitle")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub fn session_exists(session_id: &str) -> bool {
    if !safe_name_id(session_id) {
        return false;
    }
    let projects = claude_home().join("projects");
    find_first(&[projects.clone()], &format!("{session_id}.jsonl")).is_some()
        || find_dir(&[projects], session_id).is_some()
}

pub fn session_label(session_id: &str) -> SessionLabel {
    if !safe_name_id(session_id) {
        return SessionLabel::default();
    }
    let projects = claude_home().join("projects");
    let name = find_dir(&[projects.clone()], session_id)
        .and_then(|dir| std::fs::read_to_string(dir.join("custom-title.json")).ok())
        .and_then(|body| name_from_sidecar(&body))
        .and_then(|s| clip(&s));
    let first_prompt = find_first(&[projects], &format!("{session_id}.jsonl"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|body| first_user_prompt(&body));
    SessionLabel { name, first_prompt }
}

fn first_user_prompt(body: &str) -> Option<String> {
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if entry.get("type").and_then(|v| v.as_str()) != Some("user") || entry.get("isMeta") == Some(&serde_json::Value::Bool(true)) {
            continue;
        }
        if let Some(text) = entry.get("message").and_then(json_text) {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_custom_title() {
        assert_eq!(name_from_sidecar(r#"{"customTitle":"abc"}"#).as_deref(), Some("abc"));
    }

    #[test]
    fn empty_title_is_none() {
        assert_eq!(name_from_sidecar(r#"{"customTitle":"  "}"#), None);
    }

    #[test]
    fn skips_meta_and_slash_commands() {
        let body = r#"
{"type":"user","isMeta":true,"message":{"content":"caveat"}}
{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}
{"type":"user","message":{"role":"user","content":"可口可乐"}}
"#;
        assert_eq!(first_user_prompt(body).as_deref(), Some("可口可乐"));
    }
}
