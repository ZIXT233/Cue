use regex::Regex;
use std::path::{Path, PathBuf};

pub struct WindowsLaunch {
    pub executable: String,
    pub args: Vec<String>,
}

/// A `.cmd` shim whose bootstrap only wraps cmd → powershell → node (the
/// official Cursor CLI installer layout). Launching node on the script
/// directly keeps the same target minus two interpreter startups, which cost
/// seconds per launch on Windows.
pub struct DirectNodeLaunch {
    pub node: String,
    pub script: String,
    /// Applied by the caller only for keys the session env does not carry yet,
    /// matching the bootstrap's own "if unset" guards.
    pub env: Vec<(String, String)>,
}

pub fn direct_node_launch(shim: &str) -> Option<DirectNodeLaunch> {
    if !shim.to_ascii_lowercase().ends_with(".cmd") {
        return None;
    }
    let dir = Path::new(shim).parent()?.to_path_buf();
    let (node, script) = if dir.join("node.exe").is_file() && dir.join("index.js").is_file() {
        (dir.join("node.exe"), dir.join("index.js"))
    } else {
        let version = latest_cursor_version(&dir)?;
        let script = version.join("index.js");
        if !script.is_file() {
            return None;
        }
        // Each installed version carries its own node.exe; the bootstrap never
        // consults PATH. Fall back to PATH node only if an update was interrupted.
        let bundled = version.join("node.exe");
        let node = if bundled.is_file() { bundled } else { which::which("node").ok()?.into() };
        (node, script)
    };
    let mut env = vec![(
        "CURSOR_INVOKED_AS".into(),
        shim.rsplit(['\\', '/']).next().unwrap_or(shim).to_string(),
    )];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        env.push((
            "NODE_COMPILE_CACHE".into(),
            PathBuf::from(local).join("cursor-compile-cache").to_string_lossy().into_owned(),
        ));
    }
    Some(DirectNodeLaunch {
        node: node.to_string_lossy().into_owned(),
        script: script.to_string_lossy().into_owned(),
        env,
    })
}

/// Newest `versions\<date>-<hash>` directory, same YYYYMMDD integer the
/// official bootstrap sorts by. Build timestamps inside the name break ties.
fn latest_cursor_version(dir: &Path) -> Option<PathBuf> {
    let re = Regex::new(r"^\d{4}\.\d{1,2}\.\d{1,2}(-\d{2}-\d{2}-\d{2})?-[a-f0-9]+$").unwrap();
    let mut best: Option<(i64, String, PathBuf)> = None;
    for entry in std::fs::read_dir(dir.join("versions")).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !re.is_match(name) {
            continue;
        }
        let Some(stamp) = version_stamp(name) else { continue };
        if best.as_ref().is_none_or(|b| (stamp, name.to_string()) > (b.0, b.1.clone())) {
            best = Some((stamp, name.to_string(), path));
        }
    }
    best.map(|(_, _, path)| path)
}

fn version_stamp(name: &str) -> Option<i64> {
    let mut parts = name.split('-').next()?.split('.');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(year * 10_000 + month * 100 + day)
}

pub fn windows_command(file: &str, args: &[String]) -> WindowsLaunch {
    if !Regex::new(r"(?i)\.(cmd|bat)$").unwrap().is_match(file) {
        return WindowsLaunch { executable: file.to_string(), args: args.to_vec() };
    }
    // Node/cross-spawn wraps `cmd /d /s /c "…"` and sets windowsVerbatimArguments so the
    // quotes stay literal. portable-pty always CreateProcess-quotes each argv, which turns
    // that into `\"…\"` and cmd tries to run a path that literally starts with \".
    // Pass path + args as separate tokens after `/c call` instead.
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into());
    let mut launch = vec![
        "/d".into(),
        "/s".into(),
        "/c".into(),
        "call".into(),
        file.replace('/', "\\"),
    ];
    launch.extend(args.iter().cloned());
    WindowsLaunch { executable: comspec, args: launch }
}

pub fn windows_hook_command(node: &str, hook: &str, event: Option<&str>, extra_env: &[(&str, &str)]) -> String {    let literal = |value: &str| format!("'{}'", value.replace('\'', "''"));
    let mut invoke = vec![literal(node), literal(hook)];
    if let Some(event) = event { invoke.push(literal(event)); }
    let cursor_reply = extra_env.iter().find(|(k, _)| *k == "CUE_HARNESS_KIND").and_then(|(_, v)| {
        if *v == "cursor" { Some(cursor_hook_stdout(event)) } else { None }
    });
    let mut script = vec![
        "$ErrorActionPreference = 'Stop'".into(),
        "$OutputEncoding = [System.Text.UTF8Encoding]::new($false)".into(),
        "[Console]::InputEncoding = $OutputEncoding".into(),
        "[Console]::OutputEncoding = $OutputEncoding".into(),
    ];
    for (key, value) in extra_env {
        script.push(format!("$env:{key} = {}", literal(value)));
    }
    if let Some(reply) = &cursor_reply {
        script.push(format!("[Console]::Out.WriteLine('{}')", reply));
        script.push("[Console]::Out.Flush()".into());
    }
    script.push("$payload = [Console]::In.ReadToEnd()".into());
    if cursor_reply.is_some() {
        script.push(format!("$payload | & {} | Out-Null", invoke.join(" ")));
    } else {
        script.push(format!("$payload | & {}", invoke.join(" ")));
    }
    script.push("exit $LASTEXITCODE".into());
    let joined = script.join("; ");
    let encoded = utf16_le_base64(&joined);
    format!("powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {encoded}")
}

pub fn cursor_hook_stdout(event: Option<&str>) -> &'static str {
    match event {
        Some("beforeSubmitPrompt") => r#"{"continue":true}"#,
        Some("preToolUse") | Some("beforeShellExecution") | Some("beforeMCPExecution") => r#"{"permission":"allow"}"#,
        _ => "{}",
    }
}

fn utf16_le_base64(value: &str) -> String {
    let bytes: Vec<u8> = value.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_shim_uses_call_tokens_without_wrapped_quotes() {
        let launch = windows_command(r"C:\Users\ZIXT\AppData\Roaming\npm\opencode.CMD", &[]);
        assert!(launch.executable.to_ascii_lowercase().contains("cmd"));
        assert_eq!(
            launch.args,
            vec!["/d", "/s", "/c", "call", r"C:\Users\ZIXT\AppData\Roaming\npm\opencode.CMD"]
        );
        assert!(!launch.args.iter().any(|a| a.contains("\\\"") || (a.starts_with('"') && a.ends_with('"'))));
    }

    #[test]
    fn exe_passthrough() {
        let launch = windows_command(r"C:\tools\opencode.exe", &["--session".into(), "abc".into()]);
        assert_eq!(launch.executable, r"C:\tools\opencode.exe");
        assert_eq!(launch.args, vec!["--session", "abc"]);
    }

    #[test]
    fn direct_launch_resolves_newest_cursor_version() {
        let root = std::env::temp_dir().join(format!("cue-cursor-{}", std::process::id()));
        let versions = root.join("versions");
        std::fs::create_dir_all(versions.join("2026.08.01-aaaa1111")).unwrap();
        std::fs::create_dir_all(versions.join("2026.09.10-bbbb2222")).unwrap();
        std::fs::create_dir_all(versions.join("not-a-version")).unwrap();
        std::fs::write(versions.join("2026.09.10-bbbb2222").join("node.exe"), "").unwrap();
        std::fs::write(versions.join("2026.09.10-bbbb2222").join("index.js"), "").unwrap();
        std::fs::write(versions.join("2026.08.01-aaaa1111").join("index.js"), "").unwrap();
        let shim = root.join("cursor-agent.cmd");
        std::fs::write(&shim, "").unwrap();
        let direct = direct_node_launch(shim.to_str().unwrap()).expect("direct launch");
        assert!(direct.node.contains("2026.09.10-bbbb2222") && direct.node.ends_with("node.exe"));
        assert!(direct.script.contains("2026.09.10-bbbb2222") && direct.script.ends_with("index.js"));
        assert_eq!(direct.env.iter().find(|(k, _)| k == "CURSOR_INVOKED_AS").map(|(_, v)| v.as_str()), Some("cursor-agent.cmd"));
        assert!(direct.env.iter().any(|(k, _)| k == "NODE_COMPILE_CACHE"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn direct_launch_skips_non_shims_and_broken_layouts() {
        assert!(direct_node_launch(r"C:\tools\opencode.exe").is_none());
        let root = std::env::temp_dir().join(format!("cue-cursor-empty-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let shim = root.join("cursor-agent.cmd");
        std::fs::write(&shim, "").unwrap();
        assert!(direct_node_launch(shim.to_str().unwrap()).is_none(), "no versions dir yet");
        let version = root.join("versions").join("2026.09.10-bbbb2222");
        std::fs::create_dir_all(&version).unwrap();
        assert!(direct_node_launch(shim.to_str().unwrap()).is_none(), "version dir without index.js");
        let _ = std::fs::remove_dir_all(root);
    }
}
