//! One file per harness (or per family that shares a layout), each owning everything
//! about that harness: how it is launched, which hooks it is asked to report, and how
//! its hook config is written.
//!
//! Only what two or more harnesses share lives outside these files — the mechanics of
//! getting files onto a machine and naming the ingress (`super::install`), and the JSON
//! config inheritance they all lean on (`super::inherited`). A harness-private quirk
//! (Cursor's owned-entry cleanup, Grok's `cueManaged` marker, Antigravity's guard) stays
//! in the file of the harness it belongs to.

use crate::error::{AppError, AppResult};
use crate::models::QueueWorkspace;
use std::collections::HashMap;
use std::path::Path;

pub(super) mod antigravity;
pub(super) mod claude;
pub(super) mod codex;
pub(super) mod cursor;
pub(super) mod grok;
pub(super) mod opencode;
pub(super) mod pi;

use super::install::Host;
pub(super) use super::install::GlobalCtx;

/// One harness's launch shape: what to run, and how to ask it to resume a session.
pub struct Adapter {
    pub id: &'static str,
    pub executable: &'static str,
    pub args: &'static [&'static str],
    resume: fn(&str) -> AppResult<Vec<String>>,
}

impl Adapter {
    pub fn resume_args(&self, session_id: &str) -> AppResult<Vec<String>> {
        (self.resume)(session_id)
    }
}

/// Every harness resumes by handing a session id back on the command line, so the id is
/// checked once, here, before any harness sees it.
pub(super) fn checked_id(session_id: &str) -> AppResult<&str> {
    if !regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$").unwrap().is_match(session_id) {
        return Err(AppError::msg("无效的会话 ID，无法续接"));
    }
    Ok(session_id)
}

/// `--resume <id>`: what every harness without a rule of its own uses.
pub(super) fn resume_flag(session_id: &str) -> AppResult<Vec<String>> {
    Ok(vec!["--resume".into(), checked_id(session_id)?.into()])
}

fn unsupported_resume(_: &str) -> AppResult<Vec<String>> {
    Err(AppError::msg("该卡片不支持续接会话"))
}

pub fn adapter(id: &str) -> AppResult<Adapter> {
    for harness in [claude::harness, cursor::harness, codex::harness, antigravity::harness, grok::harness, opencode::harness, pi::harness] {
        if let Some(adapter) = harness(id) {
            return Ok(adapter);
        }
    }
    // A shell card runs no CLI at all: its state comes off the PTY, and there is no
    // session of its own to resume.
    if id == "shell" {
        return Ok(Adapter { id: "shell", executable: "", args: &[], resume: unsupported_resume });
    }
    Err(AppError::msg("不支持的 CLI agent"))
}

/// Everything a harness module may look at while describing its hook install.
pub struct Ctx<'a> {
    pub kind: &'a str,
    pub workspace: &'a QueueWorkspace,
    pub host: &'a Host,
    /// Where the ingress scripts are read from: a harness that needs its own looks
    /// for it here.
    pub bin_dir: &'a Path,
}

/// What one harness wants written, launched and merged.
#[derive(Default)]
pub struct Plan {
    /// Files under the plugin root, by relative name.
    pub files: HashMap<String, String>,
    /// Extra argv the CLI has to be launched with.
    pub args: Vec<String>,
    /// Extra environment the CLI has to see.
    pub env: HashMap<String, String>,
    /// User-level config files this harness merges into instead of owning.
    pub user_config: UserConfig,
}

/// Config files a harness shares with the CLI's own settings.
#[derive(Default)]
pub struct UserConfig {
    /// `$GROK_HOME/hooks/cue-session-state.json`.
    pub grok: Option<String>,
    /// `~/.gemini/config/hooks.json`.
    pub antigravity: Option<String>,
    /// `~/.cursor/hooks.json`.
    pub cursor: Option<String>,
}

/// Hand the work to the harness that owns this kind.
pub async fn plan_for(ctx: &Ctx<'_>) -> AppResult<Plan> {
    match ctx.kind {
        "codex" => codex::plan(ctx).await,
        "cursor" => cursor::plan(ctx).await,
        "antigravity" | "gemini" => antigravity::plan(ctx).await,
        "grok" => grok::plan(ctx).await,
        "opencode" => opencode::plan(ctx).await,
        "pi" | "omp" => pi::plan(ctx).await,
        // Claude's plugin layout is the default: CodeBuddy keeps it and only resolves a
        // different manifest directory first.
        _ => claude::plan(ctx).await,
    }
}
