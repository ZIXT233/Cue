use super::install::{GlobalCtx, Host};
use super::registry::{self, Ctx};
use crate::error::AppResult;
use crate::models::QueueWorkspace;
use crate::paths::{atomic_write, signal_dir};
use std::collections::HashMap;
use std::path::Path;

pub struct HookLaunch {
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

pub async fn prepare_hook_launch(
    kind: &str,
    directory: &Path,
    workspace: &QueueWorkspace,
    token: &str,
    bin_dir: &Path,
) -> AppResult<HookLaunch> {
    if workspace.kind == "local" {
        std::fs::create_dir_all(directory)?;
    }
    // The ingress is read first: a missing one has to fail the launch, and its bytes
    // also key the remote cache path.
    let ingress = std::fs::read_to_string(bin_dir.join("harness-hook.cjs"))?;
    let harness = registry::find(kind).ok_or_else(|| crate::error::AppError::msg("不支持的 CLI agent"))?;
    let host = Host::open(kind, workspace, token, &ingress).await?;
    let plan = harness.plan(Ctx { kind, workspace, host: &host, bin_dir }).await?;
    host.install(&plan).await?;
    let args = plan.args;
    let mut env = plan.env;
    env.extend(host.base_env(directory));
    if !host.remote {
        let _ = signal_dir(token);
    }
    Ok(HookLaunch { args, env })
}

/// Bring plugin copies that already exist up to the running build, and report which
/// kinds were rewritten.
///
/// `prepare_hook_launch` is the only writer, and it only runs when a card of that kind
/// launches. So a build that ships a new ingress script leaves every kind it no longer
/// launches on the old one — Cursor in particular keeps a *current* `~/.cursor/hooks.json`
/// pointing at a stale `hook.cjs`, which fails silently. Startup closes that gap for
/// local copies; remote (SSH) copies still wait for their card, because reaching them
/// would mean opening a connection this must not start on its own.
///
/// Only `hook.cjs` is aligned: the config files are merged at launch, so a changed
/// *event list* still lands the next time that kind is launched.
pub fn sync_installed_hooks(bin_dir: &Path, plugins: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(bin_dir.join("harness-hook.cjs")) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(plugins) else { return Vec::new() };
    let mut refreshed: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let kind = entry.file_name().to_str()?.to_string();
            let hook = entry.path().join("hook.cjs");
            // A kind with no copy here was never installed: its card launch decides the
            // layout, and inventing one now would guess at what the launcher writes.
            if !hook.is_file() || std::fs::read_to_string(&hook).is_ok_and(|installed| installed == source) {
                return None;
            }
            atomic_write(&hook, &source).ok().map(|()| kind)
        })
        .collect();
    refreshed.sort();
    refreshed
}

/// Install the hooks that serve sessions Cue never launched — IDE chats, plain terminals.
/// Their user-level config is global, so one pass covers all of them. Each harness owns
/// its own file (`kinds/<kind>.rs::global`); nothing here knows one from another.
pub fn deploy_external_hooks(bin_dir: &Path, plugins: &Path) {
    let ctx = GlobalCtx::new(bin_dir, plugins);
    for harness in registry::ALL {
        harness.global(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn startup_realigns_only_the_copies_that_exist() {
        let bin = tempfile::tempdir().unwrap();
        let plugins = tempfile::tempdir().unwrap();
        let source = bin.path().join("harness-hook.cjs");
        write(&source, "new ingress");
        write(&plugins.path().join("cursor/hook.cjs"), "old ingress");
        write(&plugins.path().join("codex/hook.cjs"), "new ingress");
        // A kind that was never installed here keeps its directory and nothing else.
        std::fs::create_dir_all(plugins.path().join("pi")).unwrap();
        std::fs::write(plugins.path().join("notes.txt"), "not a plugin").unwrap();

        assert_eq!(sync_installed_hooks(bin.path(), plugins.path()), ["cursor"]);
        assert_eq!(std::fs::read_to_string(plugins.path().join("cursor/hook.cjs")).unwrap(), "new ingress");
        // Up to date copies are left alone, so a second launch writes nothing.
        assert!(sync_installed_hooks(bin.path(), plugins.path()).is_empty());
        assert!(!plugins.path().join("pi/hook.cjs").exists());
    }

    #[test]
    fn a_missing_source_or_root_is_not_a_failure() {
        let bin = tempfile::tempdir().unwrap();
        let plugins = tempfile::tempdir().unwrap();
        write(&plugins.path().join("cursor/hook.cjs"), "old ingress");
        assert!(sync_installed_hooks(bin.path(), plugins.path()).is_empty());
        write(&bin.path().join("harness-hook.cjs"), "new ingress");
        assert!(sync_installed_hooks(bin.path(), &plugins.path().join("absent")).is_empty());
        assert_eq!(std::fs::read_to_string(plugins.path().join("cursor/hook.cjs")).unwrap(), "old ingress");
    }
}
