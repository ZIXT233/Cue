use super::label_text::{clip, SessionLabel};
use super::session_find::{find_all, find_dir, safe_name_id};
use std::path::{Path, PathBuf};

/// Bound the walk: keeps conversation history up to 1000 turns.
const TAIL_TURNS: usize = 1_000;
/// Memory guard for one message: allow large code blocks and detailed responses.
const MAX_TURN_CHARS: usize = 64_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// One message of the session's record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}

/// What Cursor's session record says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CursorSession {
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// Most recent prompt in the CLI's own input history or transcript.
    pub last_prompt: Option<String>,
    /// The conversation itself, oldest first, straight from the session's live transcript.
    pub turns: Vec<Turn>,
}

pub(crate) fn title_from_meta(body: &str) -> Option<String> {
    meta_text(body, "title").and_then(|s| clip(&s))
}

fn meta_text(body: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn cursor_chats() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cursor").join("chats")
}

fn cursor_projects() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cursor").join("projects")
}

/// Locate the session's live JSONL transcript in `~/.cursor/projects/*/agent-transcripts/<id>/<id>.jsonl`.
pub fn find_transcript_file(session_id: &str) -> Option<PathBuf> {
    if !safe_name_id(session_id) {
        return None;
    }
    let target = format!("{session_id}.jsonl");
    let roots = [cursor_projects(), cursor_chats()];
    let mut files = find_all(&roots, &target);
    // The session might exist under multiple project folders; pick the most recently modified one.
    files.sort_by(|a, b| {
        let time_a = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let time_b = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        time_b.cmp(&time_a)
    });
    files.into_iter().next()
}

pub fn session_exists(session_id: &str) -> bool {
    safe_name_id(session_id) && (
        find_transcript_file(session_id).is_some() ||
        find_dir(&[cursor_chats()], session_id).is_some()
    )
}

pub fn session_label(session_id: &str) -> SessionLabel {
    let dir = session_dir(session_id);
    let name = dir.as_deref().and_then(read_meta).and_then(|b| title_from_meta(&b));
    let first_prompt = find_transcript_file(session_id)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|body| first_user_prompt(&body))
        .or_else(|| {
            dir.as_deref()
                .and_then(|d| read_file(&d.join("prompt_history.json")))
                .and_then(|b| first_prompt_from_history(&b))
        });
    SessionLabel { name, first_prompt }
}

/// The session's live record: title, workspace directory and full conversation history.
pub fn session_file(session_id: &str) -> Option<CursorSession> {
    let transcript_path = find_transcript_file(session_id);
    let dir = session_dir(session_id);
    if transcript_path.is_none() && dir.is_none() {
        return None;
    }

    let meta = dir.as_deref().and_then(read_meta);
    let turns = transcript_path
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|body| parse_transcript(&body))
        .unwrap_or_default();

    let last_prompt_fallback = dir.as_deref()
        .and_then(|d| read_file(&d.join("prompt_history.json")))
        .and_then(|b| newest_prompt_from_history(&b));

    Some(CursorSession {
        title: meta.as_deref().and_then(title_from_meta),
        cwd: meta.as_deref().and_then(|b| meta_text(b, "cwd")),
        last_prompt: last_prompt_fallback,
        turns,
    })
}

fn session_dir(session_id: &str) -> Option<PathBuf> {
    safe_name_id(session_id).then(|| find_dir(&[cursor_chats()], session_id)).flatten()
}

fn read_meta(dir: &Path) -> Option<String> {
    read_file(&dir.join("meta.json"))
}

fn read_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// In Cursor's `prompt_history.json`, items are prepend-ordered (index 0 is newest).
fn newest_prompt_from_history(body: &str) -> Option<String> {
    let rows = serde_json::from_str::<Vec<serde_json::Value>>(body).ok()?;
    rows.first().and_then(|row| row.as_str().and_then(clip).or_else(|| super::label_text::json_text(row)))
}

/// The earliest prompt in history (last element in prepend-ordered array).
fn first_prompt_from_history(body: &str) -> Option<String> {
    let rows = serde_json::from_str::<Vec<serde_json::Value>>(body).ok()?;
    rows.last().and_then(|row| row.as_str().and_then(clip).or_else(|| super::label_text::json_text(row)))
}

/// Parses a Cursor agent-transcript JSONL stream into an ordered list of turns.
pub fn parse_transcript(body: &str) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let role = match value.get("role").and_then(|v| v.as_str()) {
            Some("assistant") => Role::Assistant,
            Some("user") => Role::User,
            _ => continue,
        };
        let raw_text = match value.get("message").and_then(|m| m.get("content")).or_else(|| value.get("content")) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Array(arr)) => {
                let parts: Vec<&str> = arr.iter()
                    .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect();
                parts.join("\n")
            }
            _ => continue,
        };
        let text = match role {
            Role::Assistant => clean_turn(&raw_text),
            Role::User => match extract_user_query(&raw_text) {
                Some(ask) => ask,
                None => continue,
            },
        };
        if text.is_empty() {
            continue;
        }
        // Merge consecutive assistant fragments so interim tool uses don't shatter the reply.
        if role == Role::Assistant {
            if let Some(last) = turns.last_mut().filter(|t| t.role == Role::Assistant) {
                if !last.text.is_empty() {
                    last.text.push_str("\n\n");
                }
                last.text.push_str(&text);
                if last.text.len() > MAX_TURN_CHARS {
                    last.text.truncate(MAX_TURN_CHARS);
                }
                continue;
            }
        }
        turns.push(Turn { role, text });
    }
    if turns.len() > TAIL_TURNS {
        turns.drain(..turns.len() - TAIL_TURNS);
    }
    turns
}

/// Extract the true user prompt, stripping away injected system/environment context.
fn extract_user_query(raw: &str) -> Option<String> {
    if let Some(ask) = between(raw, "<user_query>", "</user_query>") {
        let ask = clean_turn(ask);
        return (!ask.is_empty()).then_some(ask);
    }
    let text = clean_turn(raw);
    (!text.is_empty() && !text.starts_with('<')).then_some(text)
}

fn first_user_prompt(body: &str) -> Option<String> {
    for line in body.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else { continue };
        if value.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        let raw_text = match value.get("message").and_then(|m| m.get("content")).or_else(|| value.get("content")) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Array(arr)) => {
                arr.iter()
                    .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => continue,
        };
        if let Some(ask) = extract_user_query(&raw_text) {
            return Some(ask);
        }
    }
    None
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let rest = text.get(text.find(open)? + open.len()..)?;
    Some(rest.get(..rest.find(close)?)?)
}

fn clean_turn(text: &str) -> String {
    text.chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .take(MAX_TURN_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_carries_title_and_workspace() {
        let body = r#"{"schemaVersion":1,"title":"Test File Session","cwd":"/Users/u/Projects/cue"}"#;
        assert_eq!(title_from_meta(body).as_deref(), Some("Test File Session"));
        assert_eq!(meta_text(body, "cwd").as_deref(), Some("/Users/u/Projects/cue"));
        assert_eq!(meta_text(r#"{"title":"  "}"#, "title"), None);
    }

    #[test]
    fn prompt_history_prepend_order() {
        let body = r#"["最新一条","/resume","你好"]"#;
        assert_eq!(newest_prompt_from_history(body).as_deref(), Some("最新一条"));
        assert_eq!(first_prompt_from_history(body).as_deref(), Some("你好"));
        assert_eq!(newest_prompt_from_history("not json"), None);
    }

    #[test]
    fn parses_realtime_transcript_stream() {
        let stream = r#"
{"role":"user","message":{"content":[{"type":"text","text":"<timestamp>Tue 9:49 PM</timestamp>\n<user_query>\npretooluse是人看之前还是看之后\n</user_query>"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"人看之前。PreToolUse 是工具真正执行前的闸门。"}]}}
{"role":"user","message":{"content":[{"type":"text","text":"<environment_context>\nsystem stuff\n</environment_context>"}]}}
{"role":"user","message":{"content":[{"type":"text","text":"测试消息"}]}}
{"role":"assistant","message":{"content":[{"type":"tool_use","name":"Grep","input":{}},{"type":"text","text":"收到。这边正常。"}]}}
{"type":"turn_ended","status":"success"}
"#;
        let turns = parse_transcript(stream);
        assert_eq!(turns.len(), 4);
        assert_eq!(turns[0], Turn { role: Role::User, text: "pretooluse是人看之前还是看之后".into() });
        assert_eq!(turns[1], Turn { role: Role::Assistant, text: "人看之前。PreToolUse 是工具真正执行前的闸门。".into() });
        // environment_context was ignored
        assert_eq!(turns[2], Turn { role: Role::User, text: "测试消息".into() });
        assert_eq!(turns[3], Turn { role: Role::Assistant, text: "收到。这边正常。".into() });
    }

    #[test]
    fn merges_consecutive_assistant_fragments() {
        let stream = r#"
{"role":"user","message":{"content":[{"type":"text","text":"commitpush"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"正在提交本轮改动并推到远程。"}]}}
{"role":"assistant","message":{"content":[{"type":"tool_use","name":"Shell","input":{}}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"已推到 origin/main：b540732。"}]}}
"#;
        let turns = parse_transcript(stream);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, Role::User);
        assert_eq!(turns[1].role, Role::Assistant);
        assert_eq!(turns[1].text, "正在提交本轮改动并推到远程。\n\n已推到 origin/main：b540732。");
    }

    #[test]
    fn extracts_first_prompt() {
        let stream = r#"
{"role":"user","message":{"content":[{"type":"text","text":"<environment_context>noise</environment_context>"}]}}
{"role":"user","message":{"content":[{"type":"text","text":"<user_query>\n第一句真正的问题\n</user_query>"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"答案"}]}}
"#;
        assert_eq!(first_user_prompt(stream).as_deref(), Some("第一句真正的问题"));
    }

    #[test]
    fn unsafe_ids_have_no_session_dir() {
        assert_eq!(session_dir("../etc"), None);
        assert_eq!(session_file("a/b"), None);
    }
}
