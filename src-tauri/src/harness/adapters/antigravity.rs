//! Antigravity and Gemini.
//!
//! They share `~/.gemini`, so the same guard protects the file both write into: Cue's
//! bundle lives under its own `cue-session-state` key, and a key already owned by
//! someone else is never overwritten.

use super::{Adapter, Ctx, GlobalCtx, Plan};
use crate::error::{AppError, AppResult};
use crate::harness::inherited::inherited_config;
use crate::paths::atomic_write;
use std::path::{Path, PathBuf};

pub(crate) const EVENTS: &[&str] = &["PreInvocation", "PostInvocation", "PreToolUse", "PostToolUse", "Stop"];

/// Gemini reports its own lifecycle through different names than Antigravity's CLI.
const GEMINI_EVENTS: &[&str] = &["SessionStart", "BeforeAgent", "AfterAgent", "BeforeTool", "AfterTool", "Notification"];

/// Merges Cue's bundle into `~/.gemini/config/hooks.json` on a remote host.
pub(crate) const SSH_MERGE: &str = r#"const fs=require("node:fs"),p=require("node:path"),dest=process.argv[1],src=process.argv[2];const x=fs.existsSync(dest)?JSON.parse(fs.readFileSync(dest,"utf8")):{};if(x["cue-session-state"]&&!JSON.stringify(x["cue-session-state"]).includes("/cue/"))throw Error("Hook name already owned");x["cue-session-state"]=JSON.parse(fs.readFileSync(src,"utf8"));fs.mkdirSync(p.dirname(dest),{recursive:true});fs.writeFileSync(dest+".cue.tmp",JSON.stringify(x,null,2),{mode:384});fs.renameSync(dest+".cue.tmp",dest);"#;

/// Antigravity's binary is `agy`; Gemini is its own CLI over the same config directory.
pub(crate) fn harness(id: &str) -> Option<Adapter> {
    let (id, executable) = match id {
        "antigravity" => ("antigravity", "agy"),
        "gemini" => ("gemini", "gemini"),
        _ => return None,
    };
    Some(Adapter { id, executable, args: &[], resume: resume_args })
}

fn resume_args(session_id: &str) -> AppResult<Vec<String>> {
    Ok(vec!["--conversation".into(), super::checked_id(session_id)?.into()])
}

/// What an Antigravity session Cue never launched needs: its own ingress, and Cue's bundle
/// under the key it owns in the shared `~/.gemini` config.
pub(crate) fn global(ctx: &GlobalCtx) {
    let _ = ctx.install_ingress("antigravity");
    let hook_path = ctx.hook_path("antigravity");
    let mut bundle = serde_json::Map::new();
    for &event in EVENTS {
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

pub(super) async fn plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    if ctx.kind == "gemini" { gemini_plan(ctx).await } else { antigravity_plan(ctx).await }
}

async fn antigravity_plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    let mut bundle = serde_json::Map::new();
    for &event in EVENTS {
        let hook = serde_json::json!({ "type": "command", "command": ctx.host.command(Some(event)), "timeout": ctx.host.timeout });
        // Its tool events are matcher-scoped; the rest are plain handlers.
        bundle.insert(event.into(), if event.ends_with("ToolUse") {
            serde_json::json!([{ "matcher": "*", "hooks": [hook] }])
        } else {
            serde_json::json!([hook])
        });
    }
    plan.files.insert("antigravity-hooks.json".into(), serde_json::Value::Object(bundle).to_string());
    plan.user_config.antigravity = Some(if ctx.workspace.kind == "ssh" {
        format!("{}/.gemini/config/hooks.json", ctx.host.home.as_deref().unwrap_or_default())
    } else {
        home().join(".gemini/config/hooks.json").to_string_lossy().into_owned()
    });
    Ok(plan)
}

async fn gemini_plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    // Whatever the user already had stays: this file is the CLI's own defaults file.
    let mut config = inherited_config("gemini", ctx.workspace, &ctx.host.node).await?;
    let mut hooks = config.get("hooks").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    // Gemini counts milliseconds where every other harness counts seconds.
    let timeout_ms = ctx.host.timeout * 1000;
    let command = ctx.host.command(None);
    for &event in GEMINI_EVENTS {
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

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
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
pub(crate) fn guard(path: &Path, command_for: &dyn Fn(&str) -> String) -> AppResult<serde_json::Value> {
    let config = match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => return Err(error.into()),
    };
    if !config.is_object() || config.is_array() {
        return Err(AppError::msg("Invalid Antigravity hooks configuration"));
    }
    if let Some(existing) = config.get("cue-session-state") {
        if !is_owned(existing, command_for) {
            return Err(AppError::msg("Antigravity hook 同名条目不属于 Cue，未覆盖"));
        }
    }
    Ok(config)
}
