use super::label_text::{json_text, SessionLabel};
use super::session_find::{find_first, safe_name_id};
use std::path::PathBuf;

fn pi_home() -> PathBuf {
    if let Ok(path) = std::env::var("PI_HOME") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".pi")
}

pub(crate) fn name_from_jsonl(body: &str) -> Option<String> {
    let mut name = None;
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let kind = entry.get("type").and_then(|v| v.as_str());
        name = match kind {
            Some("session_info") => entry.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_string()),
            Some("title") => entry.get("title").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_string()),
            _ => continue,
        };
    }
    name
}

pub fn session_exists(session_id: &str) -> bool {
    safe_name_id(session_id)
        && find_first(
            &[pi_home().join("agent").join("sessions")],
            &format!("*_{session_id}.jsonl"),
        )
        .is_some()
}

pub fn session_label(session_id: &str) -> SessionLabel {
    if !safe_name_id(session_id) {
        return SessionLabel::default();
    }
    let Some(body) = find_first(
        &[pi_home().join("agent").join("sessions")],
        &format!("*_{session_id}.jsonl"),
    ).and_then(|path| std::fs::read_to_string(path).ok()) else {
        return SessionLabel::default();
    };
    SessionLabel {
        name: name_from_jsonl(&body),
        first_prompt: first_user_prompt(&body),
    }
}

pub(crate) fn first_user_prompt(body: &str) -> Option<String> {
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if entry.get("type").and_then(|v| v.as_str()) != Some("message") {
            continue;
        }
        let Some(message) = entry.get("message") else { continue };
        if message.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        if let Some(text) = json_text(message) {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_session_info_wins() {
        let body = r#"
{"type":"session","id":"s1"}
{"type":"session_info","name":"first"}
{"type":"message"}
{"type":"session_info","name":"renamed"}
"#;
        assert_eq!(name_from_jsonl(body).as_deref(), Some("renamed"));
    }

    #[test]
    fn empty_name_clears() {
        let body = r#"
{"type":"session_info","name":"keep"}
{"type":"session_info","name":"  "}
"#;
        assert_eq!(name_from_jsonl(body), None);
    }

    #[test]
    fn title_record_is_name() {
        let body = r#"
{"type":"session_info","name":"  "}
{"type":"title","title":"Hello There"}
"#;
        assert_eq!(name_from_jsonl(body).as_deref(), Some("Hello There"));
    }

    #[test]
    fn first_user_message() {
        let body = r#"
{"type":"session"}
{"type":"message","message":{"role":"user","content":"nihao"}}
{"type":"message","message":{"role":"assistant","content":"ok"}}
"#;
        assert_eq!(first_user_prompt(body).as_deref(), Some("nihao"));
    }
}
