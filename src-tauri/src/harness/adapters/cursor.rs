//! Cursor Agent.
//!
//! Cursor answers its own permission hooks, so this ingress must return the JSON verdict
//! the CLI waits for, and its kind is baked into every command so a foreign IDE session
//! — which inherits neither the channel nor the signal dir — still answers correctly.
//!
//! Its user-level `hooks.json` is shared with hooks the user wrote, so Cue merges into it
//! and only ever replaces entries it can prove are its own.

use super::{Adapter, Ctx, GlobalCtx, Plan};
use crate::error::{AppError, AppResult};
use crate::paths::atomic_write;
use regex::Regex;
use std::path::PathBuf;
use std::sync::OnceLock;

pub(crate) const EVENTS: &[&str] = &[
    "sessionStart", "beforeSubmitPrompt", "preToolUse", "postToolUse", "postToolUseFailure",
    "beforeShellExecution", "beforeMCPExecution", "afterAgentResponse", "stop", "sessionEnd",
];

/// Merges Cue's entries into `~/.cursor/hooks.json` on a remote host, keeping foreign
/// ones. Mirrors `merge_user_hooks`.
pub(crate) const SSH_MERGE: &str = r#"const fs=require("node:fs"),p=require("node:path"),dest=process.argv[1],src=process.argv[2],hook=process.argv[3];const owned=c=>{if(typeof c!=="string")return false;if(c.includes(hook))return true;const m=c.match(/-EncodedCommand\s+(\S+)/);if(m){try{const s=Buffer.from(m[1],"base64").toString("utf16le");if(s.includes(hook)||/[\\/](?:harness-plugins[\\/]cursor|\.cache[\\/]cue[\\/]harness)[\\/].*hook\.cjs/.test(s))return true;}catch{}}return /[\\/](?:harness-plugins[\\/]cursor|\.cache[\\/]cue[\\/]harness)[\\/].*hook\.cjs/.test(c)};const incoming=JSON.parse(fs.readFileSync(src,"utf8"));let x=fs.existsSync(dest)?JSON.parse(fs.readFileSync(dest,"utf8")):{};if(!x||Array.isArray(x)||typeof x!=="object")throw Error("Invalid Cursor hooks configuration");const hooks={...(x.hooks&&typeof x.hooks==="object"&&!Array.isArray(x.hooks)?x.hooks:{})};for(const [event,entries] of Object.entries(incoming.hooks||{})){const cur=Array.isArray(hooks[event])?hooks[event]:[];hooks[event]=[...cur.filter(e=>!owned(e&&e.command)),...entries];}x={...x,version:1,hooks};fs.mkdirSync(p.dirname(dest),{recursive:true,mode:448});fs.writeFileSync(dest+".cue.tmp",JSON.stringify(x,null,2),{mode:384});fs.renameSync(dest+".cue.tmp",dest);"#;

pub(crate) fn harness(id: &str) -> Option<Adapter> {
    if id != "cursor" { return None; }
    Some(Adapter { id: "cursor", executable: "cursor-agent", args: &[], resume: super::resume_flag })
}

/// What a Cursor session Cue never launched needs: its own ingress, plus entries in the
/// user-level `hooks.json` that IDE chats and plain terminals read from anywhere.
pub(crate) fn global(ctx: &GlobalCtx) {
    let _ = ctx.install_ingress("cursor");
    let hook_path = ctx.hook_path("cursor");
    let mut hooks = serde_json::Map::new();
    for &event in EVENTS {
        let cmd = format!("CUE_HARNESS_KIND=cursor {} \"{}\" {}", ctx.node, hook_path, event);
        hooks.insert(event.to_string(), serde_json::json!([{ "command": cmd, "timeout": 15 }]));
    }
    let path = user_hooks_path();
    let existing = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::json!({}));
    if let Ok(merged) = merge_user_hooks(existing, &serde_json::json!({ "version": 1, "hooks": hooks }), &hook_path) {
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        let _ = atomic_write(&path, &serde_json::to_string_pretty(&merged).unwrap_or_default());
    }
}

pub(super) async fn plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    plan.files.insert(
        ".cursor-plugin/plugin.json".into(),
        serde_json::json!({ "name": "cue-session-state", "version": "1.0.0", "description": "Report this Cue terminal's lifecycle" }).to_string(),
    );
    let mut hooks = serde_json::Map::new();
    for event in EVENTS {
        hooks.insert(event.to_string(), serde_json::json!([{ "command": ctx.host.command(Some(event)), "timeout": ctx.host.timeout }]));
    }
    // The plugin's own manifest is inert: the hooks that run come from the user-level
    // file below, which is also where a foreign Cursor session finds them.
    plan.files.insert("hooks/hooks.json".into(), serde_json::json!({ "version": 1, "hooks": {} }).to_string());
    plan.files.insert("cursor-user-hooks.json".into(), serde_json::json!({ "version": 1, "hooks": hooks }).to_string());
    plan.args.extend(["--plugin-dir".into(), ctx.host.root.to_string_lossy().into_owned()]);
    plan.user_config.cursor = Some(if ctx.workspace.kind == "local" {
        user_hooks_path().to_string_lossy().into_owned()
    } else {
        // The remote sink has no card directory to report into, so its signals land under
        // this token's cards folder instead.
        plan.files.insert(format!("cards/{}/.keep", ctx.host.token), String::new());
        format!("{}/.cursor/hooks.json", ctx.host.home.as_deref().unwrap_or_default())
    });
    Ok(plan)
}

pub(crate) fn user_hooks_path() -> PathBuf {
    if let Ok(path) = std::env::var("CUE_CURSOR_HOOKS") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cursor/hooks.json")
}

fn hook_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"[\\/](?:harness-plugins[\\/]cursor|\.cache[\\/]cue[\\/]harness)[\\/].*hook\.cjs"#).unwrap())
}

/// A command is Cue's when it names this hook, or when it carries a PowerShell
/// `-EncodedCommand` that does.
pub(super) fn is_owned_command(command: &str, hook_path: &str) -> bool {
    let pattern = hook_pattern();
    if command.contains(hook_path) || pattern.is_match(command) {
        return true;
    }
    let Some(encoded) = command.split_once("-EncodedCommand").and_then(|(_, rest)| rest.split_whitespace().next()) else {
        return false;
    };
    let Ok(bytes) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded) else {
        return false;
    };
    if bytes.len() % 2 != 0 {
        return false;
    }
    let units: Vec<u16> = bytes.chunks_exact(2).map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]])).collect();
    let script = String::from_utf16_lossy(&units);
    script.contains(hook_path) || pattern.is_match(&script)
}

/// Keep every foreign entry, drop Cue's old ones, append the new ones.
pub(crate) fn merge_user_hooks(existing: serde_json::Value, incoming: &serde_json::Value, hook_path: &str) -> AppResult<serde_json::Value> {
    if !existing.is_null() && (existing.is_array() || !existing.is_object()) {
        return Err(AppError::msg("Invalid Cursor hooks configuration"));
    }
    let mut current = existing.as_object().cloned().unwrap_or_default();
    let mut hooks = current
        .get("hooks")
        .filter(|v| v.is_object() && !v.is_array())
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if let Some(incoming_hooks) = incoming.get("hooks").and_then(|v| v.as_object()) {
        for (event, entries) in incoming_hooks {
            let previous = hooks.get(event).and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let kept: Vec<serde_json::Value> = previous
                .into_iter()
                .filter(|entry| !is_owned_command(entry.get("command").and_then(|c| c.as_str()).unwrap_or(""), hook_path))
                .collect();
            let extra = entries.as_array().cloned().unwrap_or_default();
            hooks.insert(event.clone(), serde_json::Value::Array(kept.into_iter().chain(extra).collect()));
        }
    }
    current.insert("version".into(), serde_json::json!(1));
    current.insert("hooks".into(), serde_json::Value::Object(hooks));
    Ok(serde_json::Value::Object(current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_foreign_and_replaces_owned() {
        let hook = "/home/u/.cache/cue/harness/newtoken/hook.cjs";
        let existing = serde_json::json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [
                    { "command": "echo foreign" },
                    { "command": "/home/u/.cache/cue/harness/oldtoken/hook.cjs" }
                ]
            }
        });
        let incoming = serde_json::json!({
            "hooks": {
                "beforeSubmitPrompt": [{ "command": format!("CUE_HARNESS_KIND=cursor /usr/bin/node {hook} beforeSubmitPrompt") }]
            }
        });
        let merged = merge_user_hooks(existing, &incoming, hook).unwrap();
        let commands: Vec<_> = merged["hooks"]["beforeSubmitPrompt"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["command"].as_str().unwrap())
            .collect();
        assert_eq!(commands, vec![
            "echo foreign",
            "CUE_HARNESS_KIND=cursor /usr/bin/node /home/u/.cache/cue/harness/newtoken/hook.cjs beforeSubmitPrompt",
        ]);
    }

    #[test]
    fn merge_rejects_array_config() {
        let err = merge_user_hooks(serde_json::json!([]), &serde_json::json!({ "hooks": {} }), "/tmp/hook.cjs").unwrap_err();
        assert_eq!(err.to_string(), "Invalid Cursor hooks configuration");
    }
}
