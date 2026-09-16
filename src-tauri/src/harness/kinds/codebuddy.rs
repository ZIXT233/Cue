//! CodeBuddy: a real harness of its own — its own manifest directory, its own binary,
//! its own title tiers — that happens to share Claude's plugin layout and transcript
//! format. It used to ride in on Claude's registry default; it is spelled out here.

use super::claude::{claude_session_details, family_plan, EVENTS};
use super::registry::{resume_flag, Adapter, Ctx, GlobalCtx, Harness, Plan};
use super::label_text::{clip, json_text, SessionLabel};
use super::session_find::{find_first, safe_name_id};
use super::session_label::SessionFacts;
use crate::error::AppResult;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub struct CodeBuddy;

pub static CODEBUDDY: CodeBuddy = CodeBuddy;

impl Harness for CodeBuddy {
    fn id(&self) -> &'static str {
        "codebuddy"
    }
    fn adapter(&self) -> Option<Adapter> {
        Some(Adapter::new("codebuddy", &[], resume_flag))
    }

    fn events(&self) -> &'static [&'static str] {
        EVENTS
    }

    fn plan<'a>(&'a self, ctx: Ctx<'a>) -> Pin<Box<dyn Future<Output = AppResult<Plan>> + Send + 'a>> {
        Box::pin(family_plan(ctx, self.events()))
    }

    /// External CodeBuddy sessions read the Claude layout through their own manifest,
    /// so their ingress copy lives under the codebuddy plugin directory; the settings
    /// merge itself is Claude's.
    fn global(&self, ctx: &GlobalCtx) {
        let _ = ctx.install_ingress("codebuddy");
    }

    fn external_ingress(&self) -> bool {
        true
    }

    // —— the session store ——

    fn session_exists(&self, id: &str) -> Option<bool> {
        Some(session_exists(id))
    }
    fn session_label(&self, id: &str, _need_first_prompt: bool) -> Option<SessionLabel> {
        Some(session_label(id))
    }
    /// CodeBuddy keeps Claude's transcript format, so the conversation reads from the
    /// one store even though the title tiers are its own.
    fn session_details(&self, id: &str) -> Option<SessionFacts> {
        claude_session_details(id).map(|details| SessionFacts {
            name: details.title,
            cwd: details.cwd,
            prompt: details.prompt,
            reply: details.reply,
            turns: details.turns,
        })
    }
}

// —— the session store ——

fn codebuddy_home() -> PathBuf {
    if let Ok(path) = std::env::var("CODEBUDDY_CONFIG_DIR") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".codebuddy")
}

/// Mirrors CodeBuddy's `PathUtils.getHomeProjectsDir()`: transcripts live at
/// `<home>/projects/<compressed-cwd>/<session-id>.jsonl`.
fn transcript_path(session_id: &str) -> Option<PathBuf> {
    find_first(&[codebuddy_home().join("projects")], &format!("{session_id}.jsonl"))
}

fn session_exists(session_id: &str) -> bool {
    if !safe_name_id(session_id) {
        return false;
    }
    transcript_path(session_id).is_some()
}

fn session_label(session_id: &str) -> SessionLabel {
    if !safe_name_id(session_id) {
        return SessionLabel::default();
    }
    let Some(body) = transcript_path(session_id).and_then(|path| std::fs::read_to_string(path).ok()) else {
        return SessionLabel::default();
    };
    SessionLabel { name: session_title(&body), first_prompt: first_user_prompt(&body) }
}

/// CodeBuddy keeps titles as transcript entries instead of Claude's sidecar file,
/// and resolves them in the same tiered order it uses for `getEffectiveSessionTitle`:
/// last user-set title wins, then the last usable generated title, then the derived topic.
fn session_title(body: &str) -> Option<String> {
    let mut custom = None;
    let mut generated = None;
    let mut topic = None;
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if let Some(text) = title_entry(&entry, "custom-title", "customTitle") {
            custom = Some(text);
        } else if let Some(text) = generated_title_entry(&entry, "ai-title", "aiTitle") {
            generated = Some(text);
        } else if let Some(text) = generated_title_entry(&entry, "topic", "topic") {
            topic = Some(text);
        }
    }
    custom.or(generated).or(topic)
}

/// A user-set title only has to be non-empty, matching the CLI.
fn title_entry(entry: &serde_json::Value, kind: &str, key: &str) -> Option<String> {
    if entry.get("type").and_then(|v| v.as_str()) != Some(kind) {
        return None;
    }
    entry.get(key).and_then(|v| v.as_str()).and_then(clip)
}

/// Generated titles additionally have to survive `isGeneratedPlaceholderTitle`.
fn generated_title_entry(entry: &serde_json::Value, kind: &str, key: &str) -> Option<String> {
    let text = title_entry(entry, kind, key)?;
    if is_placeholder_title(&text) {
        return None;
    }
    Some(text)
}

/// CodeBuddy treats these generated values as "no title yet".
fn is_placeholder_title(value: &str) -> bool {
    let text = value.trim();
    text.is_empty()
        || text == "(No content)"
        || text == "/compact"
        || (text.starts_with("<image_local_path>") && text.ends_with("</image_local_path>"))
}

/// First real user message, used as the card-title fallback.
///
/// CodeBuddy persists `{type:"message", role:"user", content, providerData}` where the
/// meta flags live under `providerData`; Claude-compatible `{type:"user", message}`
/// transcripts are accepted too so a mixed history still resolves.
fn first_user_prompt(body: &str) -> Option<String> {
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if !is_real_user_message(&entry) {
            continue;
        }
        if let Some(text) = entry.get("content").and_then(json_text).or_else(|| entry.get("message").and_then(json_text)) {
            return Some(text);
        }
    }
    None
}

fn is_real_user_message(entry: &serde_json::Value) -> bool {
    let kind = entry.get("type").and_then(|v| v.as_str());
    if kind == Some("message") {
        if entry.get("role").and_then(|v| v.as_str()) != Some("user") {
            return false;
        }
        // Internal prompts injected by the CLI itself are not user intent.
        let provider = entry.get("providerData");
        let flag = |key: &str| provider.and_then(|v| v.get(key)).and_then(|v| v.as_bool()) == Some(true);
        if flag("isMeta") || flag("isCompactInternal") || flag("skipRun") {
            return false;
        }
        if provider.and_then(|v| v.get("agent")).and_then(|v| v.as_str()) == Some("compact") {
            return false;
        }
        // A teammate message is someone else's text, not this terminal's prompt.
        return provider
            .and_then(|v| v.get("teammateMessage"))
            .and_then(|v| v.get("from"))
            .map(|v| !v.is_string())
            .unwrap_or(true);
    }
    // Claude-compatible transcript shape, with the flag at the top level.
    kind == Some("user") && entry.get("isMeta") != Some(&serde_json::Value::Bool(true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_title_wins_over_generated() {
        let body = r#"
{"type":"ai-title","aiTitle":"生成的标题"}
{"type":"topic","topic":"话题"}
{"type":"custom-title","customTitle":"我的会话"}
"#;
        assert_eq!(session_title(body).as_deref(), Some("我的会话"));
    }

    #[test]
    fn latest_entry_of_a_tier_wins() {
        let body = r#"
{"type":"custom-title","customTitle":"旧标题"}
{"type":"custom-title","customTitle":"新标题"}
"#;
        assert_eq!(session_title(body).as_deref(), Some("新标题"));
    }

    #[test]
    fn falls_back_to_topic_and_skips_placeholders() {
        let body = r#"
{"type":"ai-title","aiTitle":"   "}
{"type":"ai-title","aiTitle":"(No content)"}
{"type":"ai-title","aiTitle":"<image_local_path>/tmp/a.png</image_local_path>"}
{"type":"topic","topic":"重构队列"}
"#;
        assert_eq!(session_title(body).as_deref(), Some("重构队列"));
    }

    #[test]
    fn placeholder_topic_yields_no_title() {
        let body = r#"
{"type":"ai-title","aiTitle":"/compact"}
{"type":"topic","topic":"(No content)"}
"#;
        assert_eq!(session_title(body), None);
    }

    /// Shape captured from a real `~/.codebuddy/projects/<slug>/<id>.jsonl`.
    #[test]
    fn reads_codebuddy_message_items() {
        let body = r#"
{"id":"a1","timestamp":"2026-09-16T01:00:00.000Z","type":"message","role":"user","content":[{"type":"input_text","text":"nihao"}],"providerData":{"agent":"cli","conversationRequestId":"req1"},"__codebuddyLocal":true,"sessionId":"01a0a62a","cwd":"c:\\Users\\ZIXT"}
{"type":"file-history-snapshot","messageId":"a1","snapshot":{},"cwd":"c:\\Users\\ZIXT"}
{"type":"reasoning","content":[],"providerData":{"agent":"cli","model":"hy4-preview-f"}}
{"type":"summary","summary":"用户打了个招呼","providerData":{"source":"initial-user-message"}}
{"type":"message","role":"assistant","content":[{"type":"output_text","text":"你好！有什么可以帮你的？"}],"message":{"usage":{"input_tokens":1}}}
{"type":"turn-metrics","durationMs":1200}
"#;
        assert_eq!(first_user_prompt(body).as_deref(), Some("nihao"));
    }

    #[test]
    fn skips_internal_codebuddy_prompts() {
        let body = r#"
{"type":"message","role":"user","providerData":{"isMeta":true},"content":[{"type":"input_text","text":"caveat"}]}
{"type":"message","role":"assistant","content":"ok"}
{"type":"message","role":"user","providerData":{"isCompactInternal":true},"content":[{"type":"input_text","text":"/compact"}]}
{"type":"message","role":"user","providerData":{"agent":"compact"},"content":[{"type":"input_text","text":"压缩"}]}
{"type":"message","role":"user","providerData":{"skipRun":true},"content":[{"type":"input_text","text":"跳过"}]}
{"type":"message","role":"user","providerData":{"teammateMessage":{"from":"alice"}},"content":[{"type":"input_text","text":"别人的消息"}]}
{"type":"message","role":"user","providerData":{"agent":"cli"},"content":[{"type":"input_text","text":"修复登陆卡顿"}]}
"#;
        assert_eq!(first_user_prompt(body).as_deref(), Some("修复登陆卡顿"));
    }

    #[test]
    fn still_reads_claude_shaped_transcripts() {
        let body = r#"
{"type":"user","isMeta":true,"message":{"content":"caveat"}}
{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"优化卡片标题"}]}}
"#;
        assert_eq!(first_user_prompt(body).as_deref(), Some("优化卡片标题"));
    }

    #[test]
    fn unsafe_ids_are_rejected() {
        assert!(!session_exists("../etc/passwd"));
        assert_eq!(session_label("a/b"), SessionLabel::default());
    }
}
