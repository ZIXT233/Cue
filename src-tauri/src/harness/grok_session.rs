use super::label_text::{clip, json_text, SessionLabel};
use super::session_find::{find_all, find_dir, safe_name_id};
use std::path::PathBuf;

fn grok_home() -> PathBuf {
    if let Ok(path) = std::env::var("GROK_HOME") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".grok")
}

pub fn session_exists(session_id: &str) -> bool {
    safe_name_id(session_id) && find_dir(&[grok_home().join("sessions")], session_id).is_some()
}

pub fn session_label(session_id: &str) -> SessionLabel {
    if !safe_name_id(session_id) {
        return SessionLabel::default();
    }
    let dir = find_dir(&[grok_home().join("sessions")], session_id);
    let name = dir.as_ref().and_then(|path| std::fs::read_to_string(path.join("summary.json")).ok()).and_then(|body| title_from_summary(&body));
    let mut first_prompt = None;
    if let Some(parent) = dir.as_ref().and_then(|path| path.parent()) {
        first_prompt = std::fs::read_to_string(parent.join("prompt_history.jsonl")).ok().and_then(|body| first_prompt_from_history(&body, session_id));
    }
    if first_prompt.is_none() {
        for path in find_all(&[grok_home().join("sessions")], "prompt_history.jsonl") {
            let Ok(body) = std::fs::read_to_string(&path) else { continue };
            first_prompt = first_prompt_from_history(&body, session_id);
            if first_prompt.is_some() {
                break;
            }
        }
    }
    SessionLabel { name, first_prompt }
}

pub(crate) fn title_from_summary(body: &str) -> Option<String> {
    let entry = serde_json::from_str::<serde_json::Value>(body).ok()?;
    entry.get("generated_title").and_then(|v| v.as_str()).and_then(clip)
}

fn first_prompt_from_history(body: &str, session_id: &str) -> Option<String> {
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if entry.get("session_id").and_then(|v| v.as_str()) != Some(session_id) {
            continue;
        }
        if let Some(text) = entry.get("prompt").and_then(|v| v.as_str()).and_then(clip) {
            return Some(text);
        }
        if let Some(text) = json_text(&entry) {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_matching_session_prompt() {
        let body = r#"
{"session_id":"a","prompt":"later"}
{"session_id":"b","prompt":"hello"}
{"session_id":"b","prompt":"second"}
"#;
        assert_eq!(first_prompt_from_history(body, "b").as_deref(), Some("hello"));
    }

    #[test]
    fn reads_generated_title() {
        assert_eq!(title_from_summary(r#"{"generated_title":"aaa","title_is_manual":true}"#).as_deref(), Some("aaa"));
        assert_eq!(title_from_summary(r#"{"generated_title":"  "}"#), None);
    }
}
