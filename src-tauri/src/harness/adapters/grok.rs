//! Grok: a bundle of its own, copied into the CLI's hook directory, which is marked so
//! Cue can tell its own file from one the user wrote.

use super::{Adapter, Ctx, GlobalCtx, Plan};
use crate::error::{AppError, AppResult};
use crate::paths::atomic_write;
use crate::ssh::ssh_exec;
use std::path::{Path, PathBuf};

pub(crate) const EVENTS: &[&str] = &[
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure",
    "Stop", "StopFailure", "StopCancelled", "Notification",
];

/// Copies the bundle into place on a remote host, refusing a file Cue does not own.
pub(crate) const SSH_COPY: &str = r#"const fs=require("node:fs"),p=require("node:path"),src=process.argv[1],dest=process.argv[2];if(fs.existsSync(dest)&&JSON.parse(fs.readFileSync(dest,"utf8")).cueManaged!==true)throw Error("Existing hook file is not owned by Cue");fs.mkdirSync(p.dirname(dest),{recursive:true,mode:448});fs.copyFileSync(src,dest);fs.chmodSync(dest,384);"#;

pub(crate) fn harness(id: &str) -> Option<Adapter> {
    if id != "grok" { return None; }
    Some(Adapter { id: "grok", executable: "grok", args: &[], resume: super::resume_flag })
}

/// What a Grok session Cue never launched needs: its own ingress, and the bundle copied
/// into the CLI's hook directory under Cue's `cueManaged` marker.
pub(crate) fn global(ctx: &GlobalCtx) {
    let _ = ctx.install_ingress("grok");
    let hook_path = ctx.hook_path("grok");
    let cmd = format!("{} \"{}\"", ctx.node, hook_path);
    let mut hooks = serde_json::Map::new();
    for &event in EVENTS {
        hooks.insert(event.into(), serde_json::json!([{ "hooks": [{ "type": "command", "command": cmd, "timeout": 2 }] }]));
    }
    let dir = ctx.home.join(".grok/hooks");
    let _ = std::fs::create_dir_all(&dir);
    let _ = atomic_write(&dir.join("cue-session-state.json"), &serde_json::json!({ "cueManaged": true, "hooks": hooks }).to_string());
}

pub(super) async fn plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    let command = ctx.host.command(None);
    let mut hooks = serde_json::Map::new();
    for &event in EVENTS {
        hooks.insert(event.into(), serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": ctx.host.timeout }] }]));
    }
    plan.files.insert("grok-hooks.json".into(), serde_json::json!({ "cueManaged": true, "hooks": hooks }).to_string());
    plan.user_config.grok = Some(if ctx.workspace.kind == "ssh" {
        let host = ctx.workspace.ssh_host.as_deref().ok_or_else(|| AppError::msg("工作区不存在"))?;
        let home = String::from_utf8_lossy(&ssh_exec(host, r#"printf "%s" "${GROK_HOME:-$HOME/.grok}""#).await?).trim().to_string();
        format!("{home}/hooks/cue-session-state.json")
    } else {
        std::env::var("GROK_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".grok"))
            .join("hooks/cue-session-state.json").to_string_lossy().into_owned()
    });
    Ok(plan)
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Write over the CLI's hook file only when it still carries Cue's marker.
pub(crate) fn owned(path: &Path) -> AppResult<()> {
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            let existing: serde_json::Value = serde_json::from_str(&raw)?;
            if existing.get("cueManaged") != Some(&serde_json::json!(true)) {
                return Err(AppError::msg("Grok hook 同名文件不属于 Cue，未覆盖"));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_unowned_file() {
        let dir = std::env::temp_dir().join(format!("cue-grok-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cue-session-state.json");
        std::fs::write(&path, r#"{"hooks":{}}"#).unwrap();
        assert!(owned(&path).is_err());
        std::fs::write(&path, r#"{"cueManaged":true}"#).unwrap();
        assert!(owned(&path).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }
}
