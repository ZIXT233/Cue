//! Antigravity and Gemini.
//!
//! They share `~/.gemini`, so the same guard protects the file both write into: Cue's
//! bundle lives under its own `cue-session-state` key, and a key already owned by
//! someone else is never overwritten. Gemini is its own CLI over that same config
//! directory, with its own event names and defaults file — so it is a harness of its
//! own here, keyed for settings under Antigravity's external-ingress entry.

use super::install::Host;
use super::registry::{checked_id, Adapter, Ctx, GlobalCtx, Harness, Plan, UserMerge};
use super::inherited::inherited_config;
use super::label_text::{clip, is_noise, SessionLabel};
use super::session_find::safe_name_id;
use super::session_label::SessionFacts;
use crate::error::{AppError, AppResult};
use crate::models::ExternalTurn;
use crate::paths::atomic_write;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

const EVENTS: &[&str] = &["PreInvocation", "PostInvocation", "PreToolUse", "PostToolUse", "Stop"];

/// Gemini reports its own lifecycle through different names than Antigravity's CLI.
const GEMINI_EVENTS: &[&str] = &["SessionStart", "BeforeAgent", "AfterAgent", "BeforeTool", "AfterTool", "Notification"];

/// Merges Cue's bundle into `~/.gemini/config/hooks.json` on a remote host.
const SSH_MERGE: &str = r#"const fs=require("node:fs"),p=require("node:path"),dest=process.argv[1],src=process.argv[2];const x=fs.existsSync(dest)?JSON.parse(fs.readFileSync(dest,"utf8")):{};if(x["cue-session-state"]&&!JSON.stringify(x["cue-session-state"]).includes("/cue/"))throw Error("Hook name already owned");x["cue-session-state"]=JSON.parse(fs.readFileSync(src,"utf8"));fs.mkdirSync(p.dirname(dest),{recursive:true});fs.writeFileSync(dest+".cue.tmp",JSON.stringify(x,null,2),{mode:384});fs.renameSync(dest+".cue.tmp",dest);"#;

fn resume_args(session_id: &str) -> AppResult<Vec<String>> {
    Ok(vec!["--conversation".into(), checked_id(session_id)?.into()])
}

async fn antigravity_plan(ctx: Ctx<'_>, events: &'static [&'static str]) -> AppResult<Plan> {
    let mut plan = Plan::default();
    let mut bundle = serde_json::Map::new();
    for &event in events {
        let hook = serde_json::json!({ "type": "command", "command": ctx.host.command(Some(event)), "timeout": ctx.host.timeout });
        // Its tool events are matcher-scoped; the rest are plain handlers.
        bundle.insert(event.into(), if event.ends_with("ToolUse") {
            serde_json::json!([{ "matcher": "*", "hooks": [hook] }])
        } else {
            serde_json::json!([hook])
        });
    }
    plan.files.insert("antigravity-hooks.json".into(), serde_json::Value::Object(bundle).to_string());
    let path = if ctx.workspace.kind == "ssh" {
        format!("{}/.gemini/config/hooks.json", ctx.host.home.as_deref().unwrap_or_default())
    } else {
        home().join(".gemini/config/hooks.json").to_string_lossy().into_owned()
    };
    plan.user_config.push(UserMerge {
        path,
        payload: "antigravity-hooks.json",
        local: merge_local,
        remote: Some((SSH_MERGE, |_host: &Host, path: &str, payload: &str| vec![path.to_string(), payload.to_string()])),
    });
    Ok(plan)
}

async fn gemini_plan(ctx: Ctx<'_>, events: &'static [&'static str]) -> AppResult<Plan> {
    let mut plan = Plan::default();
    // Whatever the user already had stays: this file is the CLI's own defaults file.
    let mut config = inherited_config("gemini", ctx.workspace, &ctx.host.node).await?;
    let mut hooks = config.get("hooks").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    // Gemini counts milliseconds where every other harness counts seconds.
    let timeout_ms = ctx.host.timeout * 1000;
    let command = ctx.host.command(None);
    for &event in events {
        let mut entries = hooks.get(event).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        entries.push(serde_json::json!({
            "hooks": [{ "type": "command", "name": format!("cue-{event}"), "command": command, "timeout": timeout_ms }]
        }));
        hooks.insert(event.into(), serde_json::Value::Array(entries));
    }
    config.insert("hooks".into(), serde_json::Value::Object(hooks));
    plan.files.insert("system-defaults.json".into(), serde_json::Value::Object(config).to_string());
    plan.env.insert("GEMINI_CLI_SYSTEM_DEFAULTS_PATH".into(), ctx.host.relative("system-defaults.json"));
    Ok(plan)
}

/// Local install of the shared config: read what is there, refusing to touch a
/// `cue-session-state` key Cue does not own, then set Cue's bundle under its key.
fn merge_local(existing: Option<&str>, payload: &str, host: &Host) -> AppResult<String> {
    let mut config = guard(existing, &|event: &str| host.command(Some(event)))?;
    if let Some(obj) = config.as_object_mut() {
        obj.insert("cue-session-state".into(), serde_json::from_str(payload)?);
    }
    serde_json::to_string_pretty(&config).map_err(Into::into)
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub struct Antigravity;

pub static ANTIGRAVITY: Antigravity = Antigravity;

impl Harness for Antigravity {
    fn id(&self) -> &'static str {
        "antigravity"
    }
    fn adapter(&self) -> Option<Adapter> {
        Some(Adapter::new("agy", &[], resume_args))
    }

    fn events(&self) -> &'static [&'static str] {
        EVENTS
    }

    fn plan<'a>(&'a self, ctx: Ctx<'a>) -> Pin<Box<dyn Future<Output = AppResult<Plan>> + Send + 'a>> {
        Box::pin(antigravity_plan(ctx, self.events()))
    }

    /// What an Antigravity session Cue never launched needs: its own ingress, and Cue's
    /// bundle under the key it owns in the shared `~/.gemini` config.
    fn global(&self, ctx: &GlobalCtx) {
        let _ = ctx.install_ingress("antigravity");
        let hook_path = ctx.hook_path("antigravity");
        let mut bundle = serde_json::Map::new();
        for &event in self.events() {
            let cmd = format!("{} \"{}\" {}", ctx.node, hook_path, event);
            let entry = serde_json::json!({ "type": "command", "command": cmd, "timeout": 2 });
            bundle.insert(event.into(), if event.ends_with("ToolUse") {
                serde_json::json!([{ "matcher": "*", "hooks": [entry] }])
            } else {
                serde_json::json!([entry])
            });
        }
        let path = ctx.home.join(".gemini/config/hooks.json");
        let existing = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(mut obj) = existing.as_object().cloned() {
            obj.insert("cue-session-state".into(), serde_json::Value::Object(bundle));
            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
            let _ = atomic_write(&path, &serde_json::to_string_pretty(&serde_json::Value::Object(obj)).unwrap_or_default());
        }
    }

    /// Antigravity has no permission event at all, so a tool start is a guess: the hook
    /// runs *before* the gate, and only silence past the window tells the two apart.
    fn guesses_attention(&self, signal: &HookSignal) -> bool {
        signal.event == "PreToolUse"
    }

    /// Antigravity files stragglers once a turn really ended, so it needs the two turn
    /// boundaries told apart from the states it also reports.
    fn suppresses_stragglers(&self) -> bool {
        true
    }

    fn external_ingress(&self) -> bool {
        true
    }

    // —— the session store ——

    fn session_exists(&self, id: &str) -> Option<bool> {
        Some(session_exists(id))
    }
    fn session_label(&self, id: &str, need_first_prompt: bool) -> Option<SessionLabel> {
        let details = antigravity_session_details(id);
        Some(SessionLabel {
            name: details.as_ref().and_then(|d| d.title.clone()),
            first_prompt: if need_first_prompt { details.and_then(|d| d.prompt) } else { None },
        })
    }
    fn session_details(&self, id: &str) -> Option<SessionFacts> {
        antigravity_session_details(id).map(|details| SessionFacts {
            name: details.title,
            cwd: details.cwd,
            prompt: details.prompt,
            reply: details.reply,
            turns: details.turns,
        })
    }
}

pub struct Gemini;

pub static GEMINI: Gemini = Gemini;

impl Harness for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }
    /// External sessions over this config report through Antigravity's ingress, so the
    /// settings toggle is shared too.
    fn ingress_key(&self) -> &'static str {
        "antigravity"
    }
    fn adapter(&self) -> Option<Adapter> {
        Some(Adapter::new("gemini", &[], resume_args))
    }

    fn events(&self) -> &'static [&'static str] {
        GEMINI_EVENTS
    }

    fn plan<'a>(&'a self, ctx: Ctx<'a>) -> Pin<Box<dyn Future<Output = AppResult<Plan>> + Send + 'a>> {
        Box::pin(gemini_plan(ctx, self.events()))
    }

    // Gemini shares Antigravity's brain directory, so its label and transcript read
    // the same way — but its session existence was never claimed, and still is not.
    fn session_label(&self, id: &str, need_first_prompt: bool) -> Option<SessionLabel> {
        let details = antigravity_session_details(id);
        Some(SessionLabel {
            name: details.as_ref().and_then(|d| d.title.clone()),
            first_prompt: if need_first_prompt { details.and_then(|d| d.prompt) } else { None },
        })
    }
    fn session_details(&self, id: &str) -> Option<SessionFacts> {
        antigravity_session_details(id).map(|details| SessionFacts {
            name: details.title,
            cwd: details.cwd,
            prompt: details.prompt,
            reply: details.reply,
            turns: details.turns,
        })
    }
}

use super::signals::HookSignal;

// —— the session store ——

fn antigravity_brain_dir() -> PathBuf {
    if let Ok(path) = std::env::var("GEMINI_BRAIN_DIR") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".gemini/antigravity/brain")
}

fn locate_transcript(session_id: &str) -> Option<PathBuf> {
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

fn session_exists(session_id: &str) -> bool {
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

fn antigravity_session_details(session_id: &str) -> Option<AntigravitySessionDetails> {
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

/// A `cue-session-state` bundle is Cue's when every definition in it names a known
/// event and a handler that runs this hook.
fn is_owned(existing: &serde_json::Value, command_for: &dyn Fn(&str) -> String) -> bool {
    let Some(obj) = existing.as_object() else { return false };
    let definitions: Vec<_> = obj.iter().filter(|(key, _)| *key != "enabled").collect();
    !definitions.is_empty()
        && definitions.iter().all(|(event, entries)| {
            EVENTS.contains(&event.as_str())
                && entries.as_array().is_some_and(|items| {
                    !items.is_empty()
                        && items.iter().all(|entry| {
                            let handlers = entry
                                .get("hooks")
                                .and_then(|v| v.as_array())
                                .cloned()
                                .unwrap_or_else(|| vec![entry.clone()]);
                            !handlers.is_empty()
                                && handlers.iter().all(|handler| {
                                    handler.get("command").and_then(|c| c.as_str()).is_some_and(|command| {
                                        command == command_for(event)
                                            || regex::Regex::new(r#"[\\/]\.cue[\\/]harness-plugins[\\/]antigravity[\\/]hook\.cjs["']"#)
                                                .unwrap()
                                                .is_match(command)
                                    })
                                })
                        })
                })
        })
}

/// Read the shared config, refusing to touch a `cue-session-state` key Cue does not own.
fn guard(existing: Option<&str>, command_for: &dyn Fn(&str) -> String) -> AppResult<serde_json::Value> {
    let config = match existing {
        Some(raw) => serde_json::from_str(raw)?,
        None => serde_json::json!({}),
    };
    if !config.is_object() || config.is_array() {
        return Err(AppError::machine_detail("HARNESS_HOOKS_INVALID", "antigravity"));
    }
    if let Some(existing) = config.get("cue-session-state") {
        if !is_owned(existing, command_for) {
            return Err(AppError::machine_detail("HARNESS_HOOKS_FOREIGN", "antigravity"));
        }
    }
    Ok(config)
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
        let details = details_from_body(sample);
        assert_eq!(details.turns.len(), 2);
        assert_eq!(details.turns[0].role, "user");
        assert_eq!(details.turns[0].text, "Hello Antigravity");
        assert_eq!(details.turns[1].role, "assistant");
        assert_eq!(details.turns[1].text, "Hello! How can I help?");
        assert_eq!(details.prompt.as_deref(), Some("Hello Antigravity"));
        assert_eq!(details.reply.as_deref(), Some("Hello! How can I help?"));
        assert_eq!(details.title.as_deref(), Some("Hello Antigravity"));
    }

    fn details_from_body(body: &str) -> AntigravitySessionDetails {
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
