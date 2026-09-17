//! Grok: a bundle of its own, copied into the CLI's hook directory, which is marked so
//! Que can tell its own file from one the user wrote.

use super::install::Host;
use super::registry::{resume_flag, Adapter, Ctx, GlobalCtx, Harness, LaunchTweaks, Plan, UserMerge};
use super::label_text::{clip, json_text, SessionLabel};
use super::session_find::{find_all, find_dir, safe_name_id};
use crate::error::{AppError, AppResult};
use crate::paths::atomic_write;
use crate::ssh::ssh_exec;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

const EVENTS: &[&str] = &[
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure",
    "Stop", "StopFailure", "StopCancelled", "Notification",
];

/// Copies the bundle into place on a remote host, refusing a file Que does not own.
const SSH_COPY: &str = r#"const fs=require("node:fs"),p=require("node:path"),src=process.argv[1],dest=process.argv[2];if(fs.existsSync(dest)&&JSON.parse(fs.readFileSync(dest,"utf8")).queManaged!==true)throw Error("Existing hook file is not owned by Que");fs.mkdirSync(p.dirname(dest),{recursive:true,mode:448});fs.copyFileSync(src,dest);fs.chmodSync(dest,384);"#;

async fn plan(ctx: Ctx<'_>, events: &'static [&'static str]) -> AppResult<Plan> {
    let mut plan = Plan::default();
    let command = ctx.host.command(None);
    let mut hooks = serde_json::Map::new();
    for &event in events {
        hooks.insert(event.into(), serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": ctx.host.timeout }] }]));
    }
    plan.files.insert("grok-hooks.json".into(), serde_json::json!({ "queManaged": true, "hooks": hooks }).to_string());
    let path = if ctx.workspace.kind == "ssh" {
        let host = ctx.workspace.ssh_host.as_deref().ok_or_else(|| AppError::msg("工作区不存在"))?;
        let home = String::from_utf8_lossy(&ssh_exec(host, r#"printf "%s" "${GROK_HOME:-$HOME/.grok}""#).await?).trim().to_string();
        format!("{home}/hooks/que-session-state.json")
    } else {
        std::env::var("GROK_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".grok"))
            .join("hooks/que-session-state.json").to_string_lossy().into_owned()
    };
    plan.user_config.push(UserMerge {
        path,
        payload: "grok-hooks.json",
        local: merge_local,
        remote: Some((SSH_COPY, |_host: &Host, path: &str, payload: &str| vec![payload.to_string(), path.to_string()])),
    });
    Ok(plan)
}

/// Grok's file is a copy, not a merge: the whole file is Que's, guarded by the marker.
fn merge_local(existing: Option<&str>, payload: &str, _host: &Host) -> AppResult<String> {
    owned(existing)?;
    Ok(payload.to_string())
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Write over the CLI's hook file only when it still carries Que's marker.
fn owned(existing: Option<&str>) -> AppResult<()> {
    match existing {
        Some(raw) => {
            let parsed: serde_json::Value = serde_json::from_str(raw)?;
            if parsed.get("queManaged") != Some(&serde_json::json!(true)) {
                return Err(AppError::machine_detail("HARNESS_HOOKS_FOREIGN", "grok"));
            }
            Ok(())
        }
        None => Ok(()),
    }
}

pub struct Grok;

pub static GROK: Grok = Grok;

impl Harness for Grok {
    fn id(&self) -> &'static str {
        "grok"
    }
    fn adapter(&self) -> Option<Adapter> {
        Some(Adapter::new("grok", &[], resume_flag))
    }

    /// Grok's own launcher spells the flag without the dashes.
    fn version_flag(&self) -> &'static str {
        "version"
    }

    fn launch_tweaks(&self) -> LaunchTweaks {
        LaunchTweaks { dark_canvas: true, ..LaunchTweaks::default() }
    }

    fn events(&self) -> &'static [&'static str] {
        EVENTS
    }

    fn plan<'a>(&'a self, ctx: Ctx<'a>) -> Pin<Box<dyn Future<Output = AppResult<Plan>> + Send + 'a>> {
        Box::pin(plan(ctx, self.events()))
    }

    /// What a Grok session Que never launched needs: its own ingress, and the bundle
    /// copied into the CLI's hook directory under Que's `queManaged` marker.
    fn global(&self, ctx: &GlobalCtx) {
        let _ = ctx.install_ingress("grok");
        let hook_path = ctx.hook_path("grok");
        let cmd = format!("{} \"{}\"", ctx.node, hook_path);
        let mut hooks = serde_json::Map::new();
        for &event in self.events() {
            hooks.insert(event.into(), serde_json::json!([{ "hooks": [{ "type": "command", "command": cmd, "timeout": 2 }] }]));
        }
        let dir = ctx.home.join(".grok/hooks");
        let _ = std::fs::create_dir_all(&dir);
        let _ = atomic_write(&dir.join("que-session-state.json"), &serde_json::json!({ "queManaged": true, "hooks": hooks }).to_string());
    }

    fn remote_root(&self, home: &str, _token: &str, _ingress_sha: &str) -> String {
        format!("{home}/.cache/que/harness-plugins/grok")
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
}

// —— the session store ——

fn grok_home() -> PathBuf {
    if let Ok(path) = std::env::var("GROK_HOME") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".grok")
}

fn session_exists(session_id: &str) -> bool {
    safe_name_id(session_id) && find_dir(&[grok_home().join("sessions")], session_id).is_some()
}

fn session_label(session_id: &str) -> SessionLabel {
    if !safe_name_id(session_id) {
        return SessionLabel::default();
    }
    let dir = find_dir(&[grok_home().join("sessions")], session_id);
    let name = dir.as_ref().and_then(|path| std::fs::read_to_string(path.join("summary.json")).ok()).and_then(|body| title_from_summary(&body));
    let mut first_prompt = None;
    if let Some(parent) = dir.as_ref().and_then(|path| path.parent()) {
        first_prompt = std::fs::read_to_string(parent.join("prompt_history.jsonl")).ok().and_then(|body| first_prompt_from_history(&body, session_id));
    }
    if first_prompt.is_none() {
        for path in find_all(&[grok_home().join("sessions")], "prompt_history.jsonl") {
            let Ok(body) = std::fs::read_to_string(&path) else { continue };
            first_prompt = first_prompt_from_history(&body, session_id);
            if first_prompt.is_some() {
                break;
            }
        }
    }
    SessionLabel { name, first_prompt }
}

fn title_from_summary(body: &str) -> Option<String> {
    let entry = serde_json::from_str::<serde_json::Value>(body).ok()?;
    entry.get("generated_title").and_then(|v| v.as_str()).and_then(clip)
}

fn first_prompt_from_history(body: &str, session_id: &str) -> Option<String> {
    for line in body.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if entry.get("session_id").and_then(|v| v.as_str()) != Some(session_id) {
            continue;
        }
        if let Some(text) = entry.get("prompt").and_then(|v| v.as_str()).and_then(clip) {
            return Some(text);
        }
        if let Some(text) = json_text(&entry) {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_matching_session_prompt() {
        let body = r#"
{"session_id":"a","prompt":"later"}
{"session_id":"b","prompt":"hello"}
{"session_id":"b","prompt":"second"}
"#;
        assert_eq!(first_prompt_from_history(body, "b").as_deref(), Some("hello"));
    }

    #[test]
    fn reads_generated_title() {
        assert_eq!(title_from_summary(r#"{"generated_title":"aaa","title_is_manual":true}"#).as_deref(), Some("aaa"));
        assert_eq!(title_from_summary(r#"{"generated_title":"  "}"#), None);
    }

    #[test]
    fn refuses_unowned_file() {
        let unowned = r#"{"hooks":{}}"#;
        assert!(owned(Some(unowned)).is_err());
        assert!(owned(Some(r#"{"queManaged":true}"#)).is_ok());
        assert!(owned(None).is_ok());
    }
}
