use super::label_text::SessionLabel;
use super::pi_session::{first_user_prompt, name_from_jsonl};
use super::session_find::{find_first, safe_name_id};
use std::path::PathBuf;

fn omp_sessions() -> PathBuf {
    if let Ok(path) = std::env::var("PI_CODING_AGENT_DIR") {
        return PathBuf::from(path).join("sessions");
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".omp").join("agent").join("sessions")
}

pub fn session_exists(session_id: &str) -> bool {
    safe_name_id(session_id)
        && find_first(&[omp_sessions()], &format!("*_{session_id}.jsonl")).is_some()
}

pub fn session_label(session_id: &str) -> SessionLabel {
    if !safe_name_id(session_id) {
        return SessionLabel::default();
    }
    let Some(body) = find_first(&[omp_sessions()], &format!("*_{session_id}.jsonl"))
        .and_then(|path| std::fs::read_to_string(path).ok())
    else {
        return SessionLabel::default();
    };
    SessionLabel {
        name: name_from_jsonl(&body),
        first_prompt: first_user_prompt(&body),
    }
}
