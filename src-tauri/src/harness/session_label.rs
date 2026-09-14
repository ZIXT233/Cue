use super::claude_session;
use super::codex_session;
use super::cursor_session;
use super::grok_session;
use super::label_text::SessionLabel;
use super::omp_session;
use super::pi_session;
use super::session_find::safe_name_id;
use super::signals::ProbeState;

pub fn session_exists(kind: &str, session_id: &str) -> Option<bool> {
    if !safe_name_id(session_id) {
        return None;
    }
    match kind {
        "codex" => codex_session::codex_session_exists(session_id),
        "cursor" => Some(cursor_session::session_exists(session_id)),
        "claude" => Some(claude_session::session_exists(session_id)),
        "pi" => Some(pi_session::session_exists(session_id)),
        "omp" => Some(omp_session::session_exists(session_id)),
        "grok" => Some(grok_session::session_exists(session_id)),
        _ => None,
    }
}

pub fn read_session_label(kind: &str, session_id: &str, need_first_prompt: bool) -> Option<SessionLabel> {
    if session_id.is_empty() || session_id.contains('/') || session_id.contains('\\') {
        return None;
    }
    match kind {
        "codex" => Some(codex_session::session_label(session_id, need_first_prompt)),
        "cursor" => Some(cursor_session::session_label(session_id)),
        "claude" => Some(claude_session::session_label(session_id)),
        "pi" => Some(pi_session::session_label(session_id)),
        "omp" => Some(omp_session::session_label(session_id)),
        "grok" => Some(grok_session::session_label(session_id)),
        _ => None,
    }
}

pub fn refresh_probe_label(state: &mut ProbeState) {
    let Some(kind) = state.kind.as_deref() else { return };
    if kind == "opencode" || kind == "shell" || state.remote {
        return;
    }
    let Some(id) = state.session_id.clone() else { return };
    let Some(label) = read_session_label(kind, &id, state.first_prompt.is_none()) else { return };
    if let Some(name) = label.name {
        state.session_name = Some(name);
    }
    if let Some(first) = label.first_prompt {
        state.first_prompt = Some(first);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_kind_is_inconclusive() {
        assert_eq!(session_exists("gemini", "abc"), None);
        assert_eq!(session_exists("opencode", "sess"), None);
    }

    #[test]
    fn missing_cursor_session_is_false() {
        assert_eq!(session_exists("cursor", "00000000-0000-0000-0000-000000000000"), Some(false));
    }
}
