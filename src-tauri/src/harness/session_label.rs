//! Reading what a harness recorded: whether a session is still on disk, what to call it,
//! and — for the harnesses that keep a conversation — what was said in it.
//!
//! `access` is the one place a harness is asked what it can read. Adding a harness used
//! to mean adding a row to three separate matches that lived in two modules; it is one
//! row here, and the parsing itself stays in that harness's own `*_session.rs`.

use super::antigravity_session;
use super::claude_session;
use super::codex_session;
use super::codebuddy_session;
use super::cursor_session::{self, Role};
use super::grok_session;
use super::label_text::SessionLabel;
use super::omp_session;
use super::pi_session;
use super::session_find::safe_name_id;
use super::signals::ProbeState;
use crate::models::ExternalTurn;

/// Single-turn content limit: allow large responses and code snippets to be displayed in full.
pub(crate) const TURN_MAX_CHARS: usize = 64_000;

/// What a session's own store knows. A store that records a conversation fills in the
/// turns; one that only knows its name leaves the rest empty.
#[derive(Default)]
pub struct SessionFacts {
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub prompt: Option<String>,
    pub reply: Option<String>,
    pub turns: Vec<ExternalTurn>,
}

/// What one harness's session store can answer.
#[derive(Clone, Copy)]
struct Access {
    exists: Option<fn(&str) -> Option<bool>>,
    label: Option<fn(&str, bool) -> Option<SessionLabel>>,
    details: Option<fn(&str) -> Option<SessionFacts>>,
}

/// The one place a harness is asked what it can read.
fn access(kind: &str) -> Option<Access> {
    let nothing = || None;
    Some(match kind {
        "codex" => Access {
            exists: Some(codex_session::codex_session_exists),
            label: Some(|id, need_first_prompt| Some(codex_session::session_label(id, need_first_prompt))),
            details: Some(codex_facts),
        },
        "cursor" => Access {
            exists: Some(|id| Some(cursor_session::session_exists(id))),
            label: Some(|id, _| Some(cursor_session::session_label(id))),
            details: Some(cursor_facts),
        },
        "claude" | "codebuddy" => {
            let codebuddy = kind == "codebuddy";
            Access {
                exists: Some(if codebuddy {
                    |id| Some(codebuddy_session::session_exists(id))
                } else {
                    |id| Some(claude_session::session_exists(id))
                }),
                label: Some(if codebuddy {
                    |id, _| Some(codebuddy_session::session_label(id))
                } else {
                    |id, _| Some(claude_session::session_label(id))
                }),
                // CodeBuddy keeps Claude's transcript format, so both read one store.
                details: Some(claude_facts),
            }
        }
        "pi" | "omp" => {
            let omp = kind == "omp";
            Access {
                exists: Some(if omp { |id| Some(omp_session::session_exists(id)) } else { |id| Some(pi_session::session_exists(id)) }),
                label: Some(if omp { |id, _| Some(omp_session::session_label(id)) } else { |id, _| Some(pi_session::session_label(id)) }),
                details: None,
            }
        }
        "grok" => Access {
            exists: Some(|id| Some(grok_session::session_exists(id))),
            label: Some(|id, _| Some(grok_session::session_label(id))),
            details: None,
        },
        "antigravity" => Access {
            exists: Some(|id| Some(antigravity_session::session_exists(id))),
            label: Some(antigravity_label),
            details: Some(antigravity_facts),
        },
        // Gemini shares Antigravity's brain directory, so its label and transcript read
        // the same way — but its session existence was never claimed, and still is not.
        "gemini" => Access { exists: nothing(), label: Some(antigravity_label), details: Some(antigravity_facts) },
        _ => return None,
    })
}

pub fn session_exists(kind: &str, session_id: &str) -> Option<bool> {
    if !safe_name_id(session_id) {
        return None;
    }
    access(kind)?.exists?(session_id)
}

pub fn read_session_label(kind: &str, session_id: &str, need_first_prompt: bool) -> Option<SessionLabel> {
    if session_id.is_empty() || session_id.contains('/') || session_id.contains('\\') {
        return None;
    }
    access(kind)?.label?(session_id, need_first_prompt)
}

/// What the session's own store knows. A store with no transcript still names its
/// session, which is all a notice needs to be recognisable.
pub fn session_facts(kind: &str, session_id: &str) -> SessionFacts {
    let Some(access) = access(kind) else { return SessionFacts::default() };
    if let Some(facts) = access.details.and_then(|details| details(session_id)) {
        return facts;
    }
    access
        .label
        .and_then(|label| label(session_id, true))
        .map(|label| SessionFacts { name: label.name, prompt: label.first_prompt, ..SessionFacts::default() })
        .unwrap_or_default()
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

fn antigravity_label(id: &str, need_first_prompt: bool) -> Option<SessionLabel> {
    let details = antigravity_session::antigravity_session_details(id);
    Some(SessionLabel {
        name: details.as_ref().and_then(|d| d.title.clone()),
        first_prompt: if need_first_prompt { details.and_then(|d| d.prompt) } else { None },
    })
}

fn antigravity_facts(id: &str) -> Option<SessionFacts> {
    let details = antigravity_session::antigravity_session_details(id)?;
    Some(SessionFacts { name: details.title, cwd: details.cwd, prompt: details.prompt, reply: details.reply, turns: details.turns })
}

fn codex_facts(id: &str) -> Option<SessionFacts> {
    let details = codex_session::codex_session_details(id)?;
    Some(SessionFacts { name: details.title, cwd: details.cwd, prompt: details.prompt, reply: details.reply, turns: details.turns })
}

fn claude_facts(id: &str) -> Option<SessionFacts> {
    let details = claude_session::claude_session_details(id)?;
    Some(SessionFacts { name: details.title, cwd: details.cwd, prompt: details.prompt, reply: details.reply, turns: details.turns })
}

/// Cursor's store is the conversation itself: the last user turn is what this session is
/// answering, and the last assistant turn is the reply a notice shows.
fn cursor_facts(id: &str) -> Option<SessionFacts> {
    let file = cursor_session::session_file(id)?;
    let last = |role: Role| file.turns.iter().rev().find(|turn| turn.role == role).map(|turn| turn.text.clone());
    Some(SessionFacts {
        name: file.title,
        cwd: file.cwd,
        // The store is this session's own record, so it wins; the CLI's input history is
        // shared with whatever conversation was resumed before it.
        prompt: last(Role::User).or(file.last_prompt),
        reply: last(Role::Assistant),
        turns: file.turns.iter().map(external_turn).collect(),
    })
}

fn external_turn(turn: &cursor_session::Turn) -> ExternalTurn {
    ExternalTurn { role: turn.role.as_str().to_string(), text: clean_text(&turn.text, TURN_MAX_CHARS).unwrap_or_default() }
}

/// Trim to what a card can show: keep the line breaks a terminal reply is built from,
/// drop everything else the hooks process may have leaked in.
pub(crate) fn clean_text(text: &str, max: usize) -> Option<String> {
    let kept: String = text.chars().filter(|c| *c == '\n' || !c.is_control()).take(max).collect();
    let mut lines: Vec<&str> = Vec::new();
    for line in kept.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() && lines.last().is_some_and(|last| last.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    let text = lines.join("\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
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
