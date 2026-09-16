use super::label_text::{clip, is_noise};
use super::session_find::safe_name_id;
use crate::models::ExternalTurn;
use std::path::PathBuf;

fn antigravity_brain_dir() -> PathBuf {
    if let Ok(path) = std::env::var("GEMINI_BRAIN_DIR") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".gemini/antigravity/brain")
}

pub fn locate_transcript(session_id: &str) -> Option<PathBuf> {
    if !safe_name_id(session_id) {
        return None;
    }
    let brain = antigravity_brain_dir();
    let candidates = [
        brain.join(session_id).join(".system_generated/logs/transcript.jsonl"),
        brain.join(session_id).join("transcript.jsonl"),
    ];
    for path in candidates {
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

pub fn session_exists(session_id: &str) -> bool {
    locate_transcript(session_id).is_some()
}

#[derive(Debug, Clone, Default)]
pub struct AntigravitySessionDetails {
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub prompt: Option<String>,
    pub reply: Option<String>,
    pub turns: Vec<ExternalTurn>,
}

pub fn antigravity_session_details(session_id: &str) -> Option<AntigravitySessionDetails> {
    let path = locate_transcript(session_id)?;
    let body = std::fs::read_to_string(path).ok()?;
    let mut turns = Vec::new();
    let mut first_user = None;
    let mut last_user = None;
    let mut last_assistant = None;

    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let step_type = entry.get("type").and_then(|v| v.as_str());
        let content = entry.get("content").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
        let Some(text) = content else { continue };

        if step_type == Some("USER_INPUT") {
            if is_noise(text) {
                continue;
            }
            if first_user.is_none() {
                first_user = Some(text.to_string());
            }
            last_user = Some(text.to_string());
            turns.push(ExternalTurn {
                role: "user".into(),
                text: text.to_string(),
            });
        } else if step_type == Some("PLANNER_RESPONSE") {
            last_assistant = Some(text.to_string());
            turns.push(ExternalTurn {
                role: "assistant".into(),
                text: text.to_string(),
            });
        }
    }

    let title = first_user.as_deref().and_then(clip);

    Some(AntigravitySessionDetails {
        title,
        cwd: None,
        prompt: last_user,
        reply: last_assistant,
        turns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_transcript_lines() {
        let sample = r#"
{"step_index":0,"type":"USER_INPUT","content":"Hello Antigravity"}
{"step_index":1,"type":"PLANNER_RESPONSE","content":"Hello! How can I help?"}
"#;
        let details = antigravity_session_details_from_body(sample);
        assert_eq!(details.turns.len(), 2);
        assert_eq!(details.turns[0].role, "user");
        assert_eq!(details.turns[0].text, "Hello Antigravity");
        assert_eq!(details.turns[1].role, "assistant");
        assert_eq!(details.turns[1].text, "Hello! How can I help?");
        assert_eq!(details.prompt.as_deref(), Some("Hello Antigravity"));
        assert_eq!(details.reply.as_deref(), Some("Hello! How can I help?"));
        assert_eq!(details.title.as_deref(), Some("Hello Antigravity"));
    }

    fn antigravity_session_details_from_body(body: &str) -> AntigravitySessionDetails {
        let mut turns = Vec::new();
        let mut first_user = None;
        let mut last_user = None;
        let mut last_assistant = None;

        for line in body.lines() {
            let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let step_type = entry.get("type").and_then(|v| v.as_str());
            let content = entry.get("content").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
            let Some(text) = content else { continue };

            if step_type == Some("USER_INPUT") {
                if is_noise(text) {
                    continue;
                }
                if first_user.is_none() {
                    first_user = Some(text.to_string());
                }
                last_user = Some(text.to_string());
                turns.push(ExternalTurn {
                    role: "user".into(),
                    text: text.to_string(),
                });
            } else if step_type == Some("PLANNER_RESPONSE") {
                last_assistant = Some(text.to_string());
                turns.push(ExternalTurn {
                    role: "assistant".into(),
                    text: text.to_string(),
                });
            }
        }

        let title = first_user.as_deref().and_then(clip);
        AntigravitySessionDetails {
            title,
            cwd: None,
            prompt: last_user,
            reply: last_assistant,
            turns,
        }
    }
}
