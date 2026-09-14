use regex::Regex;

pub struct WindowsLaunch {
    pub executable: String,
    pub args: Vec<String>,
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

pub fn windows_hook_command(node: &str, hook: &str, event: Option<&str>, extra_env: &[(&str, &str)]) -> String {
    let literal = |value: &str| format!("'{}'", value.replace('\'', "''"));
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
        Some("beforeShellExecution") | Some("beforeMCPExecution") => r#"{"permission":"ask"}"#,
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
}
