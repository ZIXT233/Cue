use regex::Regex;

pub struct WindowsLaunch {
    pub executable: String,
    pub args: Vec<String>,
}

pub fn windows_command(file: &str, args: &[String]) -> WindowsLaunch {
    if !Regex::new(r"(?i)\.(cmd|bat)$").unwrap().is_match(file) {
        return WindowsLaunch { executable: file.to_string(), args: args.to_vec() };
    }
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into());
    let double_escape = Regex::new(r"(?i)node_modules[\\/]\.bin[\\/][^\\/]+\.cmd$").unwrap().is_match(file);
    let meta = Regex::new(r#"([()\][%!^"`<>&|;, *?])"#).unwrap();
    let quoted: Vec<String> = args.iter().map(|value| {
        let mut escaped = format!("\"{}\"", value.replace('\\', "\\").replace('"', "\\\""));
        escaped = meta.replace_all(&escaped, "^$1").into_owned();
        if double_escape { escaped = meta.replace_all(&escaped, "^$1").into_owned(); }
        escaped
    }).collect();
    let command = std::iter::once(meta.replace_all(&file.replace('/', "\\"), "^$1").into_owned())
        .chain(quoted)
        .collect::<Vec<_>>()
        .join(" ");
    WindowsLaunch {
        executable: comspec,
        args: vec!["/d".into(), "/s".into(), "/c".into(), format!("\"{command}\"")],
    }
}

pub fn windows_hook_command(node: &str, hook: &str, event: Option<&str>, extra_env: &[(&str, &str)]) -> String {
    let literal = |value: &str| format!("'{}'", value.replace('\'', "''"));
    let mut invoke = vec![literal(node), literal(hook)];
    if let Some(event) = event { invoke.push(literal(event)); }
    let cursor_reply = extra_env.iter().find(|(k, _)| *k == "TOPCARD_HARNESS_KIND").and_then(|(_, v)| {
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
