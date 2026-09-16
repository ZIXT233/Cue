//! Codex: no hook files at all. Every event is handed to the CLI as a `-c` override.

use super::{Adapter, Ctx, GlobalCtx, Plan};
use crate::error::{AppError, AppResult};
use crate::paths::atomic_write;
use std::path::PathBuf;

pub(crate) const EVENTS: &[&str] = &[
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse", "Stop",
];

/// Pin the TUI to the title and notification channel Cue reads back.
const ARGS: &[&str] = &[
    "-c", r#"tui.terminal_title=["app-name","status","spinner","session-id"]"#,
    "-c", r#"tui.notifications=["plan-mode-prompt","approval-requested"]"#,
    "-c", r#"tui.notification_method="osc9""#,
    "-c", r#"tui.notification_condition="always""#,
];

pub(crate) fn harness(id: &str) -> Option<Adapter> {
    if id != "codex" { return None; }
    Some(Adapter { id: "codex", executable: "codex", args: ARGS, resume: resume_args })
}

/// Codex wants the full rollout uuid back, not just any session id.
fn resume_args(session_id: &str) -> AppResult<Vec<String>> {
    let id = super::checked_id(session_id)?;
    if !regex::Regex::new(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$").unwrap().is_match(id) {
        return Err(AppError::msg("无效的 Codex 会话 ID，无法续接"));
    }
    Ok(vec!["resume".into(), id.into()])
}

/// Cue's entries in the CLI's own config are recognised by the ingress path they run.
/// That is how an earlier install is told apart from hooks the user wrote themselves.
const MARKER: [&str; 2] = ["harness-plugins/codex/hook.cjs", "harness-plugins\\codex\\hook.cjs"];

/// What a Codex session Cue never launched needs: its own ingress, and a `config.toml`
/// that registers the hooks. Codex has no config *file* for hooks otherwise.
pub(crate) fn global(ctx: &GlobalCtx) {
    let _ = ctx.install_ingress("codex");
    let dir = std::env::var("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|_| ctx.home.join(".codex"));
    if !dir.exists() {
        return;
    }
    let config = dir.join("config.toml");
    let existing = std::fs::read_to_string(&config).unwrap_or_default();
    if MARKER.iter().any(|marker| existing.contains(marker)) {
        // Already registered by an earlier run: appending again would double every
        // event. It can still be the *shape* an earlier Cue wrote, though — that one
        // has to be rewritten in place, or Codex keeps refusing to load the file and
        // the user cannot even reach the prompt that would trust these hooks.
        let repaired = repair_headers(&existing);
        if repaired != existing {
            let _ = atomic_write(&config, &repaired);
        }
        return;
    }
    let cmd = format!("{} \"{}\"", ctx.node, ctx.hook_path("codex"));
    let mut to_append = String::new();
    if !existing.contains("[features]") {
        to_append.push_str("\n[features]\nhooks = true\n");
    }
    to_append.push_str(&registration_block(&cmd));
    // The user's own config, written atomically: a torn write here costs them every
    // Codex session, not just Cue's entries.
    let _ = atomic_write(&config, &format!("{existing}\n{to_append}"));
}

/// The hooks Cue registers, in the shape Codex parses.
///
/// Every event is an **array of matcher groups** — `[[hooks.PreToolUse]]` — and not a
/// table. With a single-bracket header the event key holds a map where Codex wants a
/// sequence, which it reports as `invalid type: map, expected a sequence in 'hooks'`
/// and treats as a reason to reject the *whole* config file.
fn registration_block(cmd: &str) -> String {
    let mut out = String::from("\n# Cue session state hook\n");
    for &event in EVENTS {
        out.push_str(&format!("[[hooks.{event}]]\nhooks = [{{ type = \"command\", command = {:?}, timeout = 2 }}]\n", cmd));
    }
    out
}

/// Rewrite the headers an earlier Cue wrote as tables into the arrays Codex wants.
///
/// Only the header moves: the `hooks = [...]` line beneath it is already the body of
/// the group, so the entry keeps doing exactly what it did. Everything else in the
/// file is left byte for byte — this is the user's config, and Cue is only a guest in
/// it. Matching on the newline on both sides is what keeps `[[hooks.Stop]]` (already
/// right) from being bracketed a third time.
fn repair_headers(existing: &str) -> String {
    let mut out = existing.to_string();
    for &event in EVENTS {
        for newline in ["\r\n", "\n"] {
            out = out.replace(
                &format!("{newline}[hooks.{event}]{newline}"),
                &format!("{newline}[[hooks.{event}]]{newline}"),
            );
        }
    }
    out
}

pub(super) async fn plan(ctx: &Ctx<'_>) -> AppResult<Plan> {
    let mut plan = Plan::default();
    let command = ctx.host.command(None);
    plan.args.extend(["--enable".into(), "hooks".into()]);
    for &event in EVENTS {
        // Same shape as `registration_block`: an array of groups, each holding the
        // command to run. An override is parsed as config, so a table here would be
        // rejected exactly like one in the file.
        plan.args.extend(["-c".into(), format!("hooks.{event}=[{{hooks=[{{type=\"command\",command={},timeout={}}}]}}]", serde_json::to_string(&command)?, ctx.host.timeout)]);
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserted per event, because one table among five arrays is enough to make Codex
    /// reject the file — and the failure then looks like a Cue problem, not a typo.
    #[test]
    fn registered_events_are_arrays_of_groups() {
        let block = registration_block("/usr/bin/node \"/tmp/cue/hook.cjs\"");
        for &event in EVENTS {
            assert!(block.contains(&format!("[[hooks.{event}]]\n")), "{event} must be an array of tables");
            assert!(!block.contains(&format!("\n[hooks.{event}]\n")), "{event} must not be a table");
        }
    }

    /// An install written before the shape was corrected is repaired in place, and the
    /// entries survive: only the header was ever wrong.
    #[test]
    fn table_headers_from_an_earlier_install_are_repaired() {
        let entry = "hooks = [{ type = \"command\", command = \"node\", timeout = 2 }]";
        let broken = format!("\n# Cue session state hook\n[hooks.Stop]\n{entry}\n");
        let fixed = repair_headers(&broken);
        assert!(fixed.contains("\n[[hooks.Stop]]\n"));
        assert!(!fixed.contains("\n[hooks.Stop]\n"));
        assert!(fixed.contains(entry), "the group itself must not be touched");
        // Repairing twice must not stack a third bracket, and a correct block is a no-op.
        assert_eq!(repair_headers(&fixed), fixed);
        let block = registration_block("node");
        assert_eq!(repair_headers(&block), block);
    }

    /// Anything that is not Cue's own header is left exactly as it was — including the
    /// CLI's own `[hooks.state]` tables, which live under the same key.
    #[test]
    fn a_repair_touches_nothing_else() {
        let theirs = "[model]\nname = \"gpt\"\n\n[hooks.state.\"/Users/x/.codex/hooks.json:stop:0:0\"]\ntrusted_hash = \"sha256:1\"\n";
        assert_eq!(repair_headers(theirs), theirs);
    }
}
