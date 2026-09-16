//! Claude Code and CodeBuddy: the same plugin layout, a different manifest directory.

use super::{Adapter, Ctx, GlobalCtx, Plan};
use crate::error::AppResult;
use crate::paths::atomic_write;

/// CodeBuddy keeps the Claude plugin layout but resolves its own manifest directory
/// first; `hooks/hooks.json` is read the same way.
fn manifest_dir(kind: &str) -> &'static str {
    if kind == "codebuddy" { ".codebuddy-plugin" } else { ".claude-plugin" }
}

/// Notification is what a blocked prompt actually reaches us through: permission
/// prompts and ask-style tools never surface as a tool call, so without it a card waits
/// forever with no signal to react to.
pub(crate) const EVENTS: &[&str] = &[
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "Notification",
    "PostToolUse", "PostToolUseFailure", "Stop", "StopFailure",
];

/// Claude Code and CodeBuddy: the same launcher, a different binary.
pub(crate) fn harness(id: &str) -> Option<Adapter> {
    let (id, executable) = match id {
        "claude" => ("claude", "claude"),
        "codebuddy" => ("codebuddy", "codebuddy"),
        _ => return None,
    };
    Some(Adapter { id, executable, args: &[], resume: super::resume_flag })
}

/// What a Claude or CodeBuddy session Cue never launched needs: their ingress (both read
/// the same layout), and Cue's entries merged into `~/.claude/settings.json`.
pub(crate) fn global(ctx: &GlobalCtx) {
    let _ = ctx.install_ingress("claude");
    let _ = ctx.install_ingress("codebuddy");
    let dir = ctx.home.join(".claude");
    // The VS Code extension keeps this file too, so a machine that never ran the CLI
    // still gets managed when its settings directory exists.
    let has_extension = [".vscode/extensions", ".vscode-insiders/extensions", ".cursor/extensions"]
        .iter()
        .any(|ext| {
            ctx.home.join(ext)
                .read_dir()
                .ok()
                .map(|entries| entries.filter_map(|entry| entry.ok()).any(|entry| entry.file_name().to_string_lossy().starts_with("anthropic.claude-code")))
                .unwrap_or(false)
        });
    if !dir.exists() && !has_extension {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let settings = dir.join("settings.json");
    let cmd = format!("{} \"{}\"", ctx.node, ctx.hook_path("claude"));
    let mut value: serde_json::Value = std::fs::read_to_string(&settings)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        let mut hooks = obj.get("hooks").and_then(|h| h.as_object()).cloned().unwrap_or_default();
        for &event in EVENTS {
            // Keep whatever the user wrote there; replace only Cue's own entries.
            let mut entries: Vec<serde_json::Value> = hooks.get(event).and_then(|v| v.as_array()).cloned().unwrap_or_default();
            entries.retain(|group| {
                let text = group.to_string();
                !text.contains("harness-plugins/claude/hook.cjs") && !text.contains("harness-plugins\\claude\\hook.cjs")
            });
            entries.push(serde_json::json!({ "hooks": [{ "type": "command", "command": cmd, "timeout": 2 }] }));
            hooks.insert(event.into(), serde_json::Value::Array(entries));
        }
        obj.insert("hooks".into(), serde_json::Value::Object(hooks));
        let _ = atomic_write(&settings, &serde_json::to_string_pretty(&serde_json::Value::Object(obj.clone())).unwrap_or_default());
    }
}

pub(super) async fn plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    plan.files.insert(
        format!("{}/plugin.json", manifest_dir(ctx.kind)),
        serde_json::json!({ "name": "cue-session-state", "version": "1.0.0", "description": "Report this Cue terminal's lifecycle" }).to_string(),
    );
    let command = ctx.host.command(None);
    let mut hooks = serde_json::Map::new();
    for &event in EVENTS {
        hooks.insert(event.to_string(), serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": ctx.host.timeout }] }]));
    }
    plan.files.insert("hooks/hooks.json".into(), serde_json::json!({ "hooks": hooks }).to_string());
    plan.args.extend(["--plugin-dir".into(), ctx.host.root.to_string_lossy().into_owned()]);
    Ok(plan)
}
