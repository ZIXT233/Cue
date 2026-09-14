use super::label_text::{clip, SessionLabel};
use super::session_find::{find_dir, safe_name_id};
use std::path::PathBuf;

pub(crate) fn title_from_meta(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn cursor_chats() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cursor").join("chats")
}

pub fn session_exists(session_id: &str) -> bool {
    safe_name_id(session_id) && find_dir(&[cursor_chats()], session_id).is_some()
}

pub fn session_label(session_id: &str) -> SessionLabel {
    if !safe_name_id(session_id) {
        return SessionLabel::default();
    }
    let Some(dir) = find_dir(&[cursor_chats()], session_id) else {
        return SessionLabel::default();
    };
    let name = std::fs::read_to_string(dir.join("meta.json")).ok().and_then(|body| title_from_meta(&body)).and_then(|s| clip(&s));
    let first_prompt = std::fs::read_to_string(dir.join("prompt_history.json")).ok().and_then(|body| first_prompt_from_history(&body));
    SessionLabel { name, first_prompt }
}

fn first_prompt_from_history(body: &str) -> Option<String> {
    let rows = serde_json::from_str::<Vec<serde_json::Value>>(body).ok()?;
    rows.last().and_then(|row| row.as_str().and_then(clip).or_else(|| super::label_text::json_text(row)))
}
