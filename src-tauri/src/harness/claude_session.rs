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

#[derive(Debug, Clone, Default)]
pub struct ClaudeSessionDetails {
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub prompt: Option<String>,
    pub reply: Option<String>,
    pub turns: Vec<crate::models::ExternalTurn>,
}

pub fn claude_session_details(session_id: &str) -> Option<ClaudeSessionDetails> {
    if !safe_name_id(session_id) {
        return None;
    }
    let projects = claude_home().join("projects");
    let label = session_label(session_id);
    let path = find_first(&[projects], &format!("{session_id}.jsonl"))?;
    let body = std::fs::read_to_string(path).ok()?;
    let mut turns = Vec::new();
    let mut last_user = None;
    let mut last_assistant = None;

    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let msg_type = entry.get("type").and_then(|v| v.as_str());
        if entry.get("isMeta") == Some(&serde_json::Value::Bool(true)) {
            continue;
        }
        let Some(raw_msg) = entry.get("message") else { continue };
        let text = match raw_msg {
            serde_json::Value::String(s) => s.trim().to_string(),
            serde_json::Value::Object(o) => {
                if let Some(s) = o.get("content").and_then(|c| c.as_str()) {
                    s.trim().to_string()
                } else if let Some(parts) = o.get("content").and_then(|c| c.as_array()) {
                    let mut joined = Vec::new();
                    for part in parts {
                        if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                            joined.push(t.trim().to_string());
                        }
                    }
                    joined.join("\n\n")
                } else {
                    json_text(raw_msg).unwrap_or_default()
                }
            }
            _ => json_text(raw_msg).unwrap_or_default(),
        };
        if text.is_empty() || super::label_text::is_noise(&text) {
            continue;
        }
        if msg_type == Some("user") {
            last_user = Some(text.clone());
            turns.push(crate::models::ExternalTurn {
                role: "user".into(),
                text,
            });
        } else if msg_type == Some("assistant") {
            last_assistant = Some(text.clone());
            turns.push(crate::models::ExternalTurn {
                role: "assistant".into(),
                text,
            });
        }
    }

    Some(ClaudeSessionDetails {
        title: label.name,
        cwd: None,
        prompt: last_user.or(label.first_prompt),
        reply: last_assistant,
        turns,
    })
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
