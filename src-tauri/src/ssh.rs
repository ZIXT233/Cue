use crate::error::{AppError, AppResult};
use crate::hosts::saved_host;
use crate::models::RemoteHost;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::process::Command;

fn socket_root() -> PathBuf {
    static DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    let slot = DIR.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().unwrap();
    if let Some(path) = guard.clone() {
        return path;
    }
    let base = if cfg!(windows) { std::env::temp_dir() } else { PathBuf::from("/tmp") };
    let path = tempfile::Builder::new().prefix("cq-s-").tempdir_in(base).map(|d| d.keep()).unwrap_or_else(|_| std::env::temp_dir());
    *guard = Some(path.clone());
    path
}

fn host_ok(value: &str) -> bool {
    regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._@:-]*$").unwrap().is_match(value)
}

pub async fn connection_args(host: &str, request_tty: bool) -> AppResult<Vec<String>> {
    if !host_ok(host) {
        return Err(AppError::machine("HOST_INVALID"));
    }
    let saved = saved_host(host)?;
    if host.starts_with("web-") && saved.is_none() {
        return Err(AppError::machine("HOST_DELETED"));
    }
    let target = saved.unwrap_or(RemoteHost {
        id: host.to_string(),
        name: host.to_string(),
        hostname: host.to_string(),
        user: None,
        port: None,
        source: "config".into(),
        visible: None,
        connected: None,
    });
    Ok(target_args(&target, request_tty))
}

pub fn target_args(target: &RemoteHost, request_tty: bool) -> Vec<String> {
    let key = serde_json::json!({
        "hostname": target.hostname,
        "user": target.user,
        "port": target.port,
        "id": target.id,
    }).to_string();
    let digest = hex::encode(&Sha256::digest(key.as_bytes())[..12]);
    let socket = socket_root().join(digest);
    let mut args = vec![
        if request_tty { "-tt" } else { "-T" }.into(),
        "-S".into(),
        socket.to_string_lossy().into_owned(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=yes".into(),
        "-o".into(),
        "ConnectTimeout=8".into(),
        "-o".into(),
        "ServerAliveInterval=15".into(),
    ];
    if let Some(port) = target.port {
        args.extend(["-p".into(), port.to_string()]);
    }
    if let Some(user) = &target.user {
        args.extend(["-l".into(), user.clone()]);
    }
    args.push(target.hostname.clone());
    args
}

pub async fn is_connected(host: &str) -> bool {
    let Ok(args) = connection_args(host, false).await else { return false };
    Command::new("ssh").arg("-O").arg("check").args(args).output().await.map(|o| o.status.success()).unwrap_or(false)
}

pub async fn connect_host(host: &str, password: Option<String>, trusted_prompt: Option<String>) -> AppResult<()> {
    let args = connection_args(host, false).await?;
    connect_args(args, password, trusted_prompt).await
}

pub async fn test_target(target: RemoteHost, password: Option<String>, trusted_prompt: Option<String>) -> AppResult<()> {
    connect_args(target_args(&target, false), password, trusted_prompt).await
}

async fn connect_args(mut args: Vec<String>, password: Option<String>, trusted_prompt: Option<String>) -> AppResult<()> {
    let check = Command::new("ssh").arg("-O").arg("check").args(&args).output().await;
    if check.map(|o| o.status.success()).unwrap_or(false) {
        return Ok(());
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| AppError::machine("SOCKET_PATH"))?;
    listener.set_nonblocking(true).ok();
    let port = listener.local_addr().map_err(|_| AppError::machine("SOCKET_PATH"))?.port();
    let challenge = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let challenge_thread = challenge.clone();
    let trusted = trusted_prompt.clone();
    let password = password.clone().unwrap_or_default();
    std::thread::spawn(move || {
        listener.set_nonblocking(false).ok();
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
            let mut buf = vec![0u8; 4096];
            if let Ok(n) = stream.read(&mut buf) {
                let prompt = String::from_utf8_lossy(&buf[..n]).into_owned();
                let reply = if regex::Regex::new(r"(?is)continue connecting[\s\S]*yes/no").unwrap().is_match(&prompt) {
                    if trusted.as_deref() == Some(prompt.as_str()) {
                        "yes\n".to_string()
                    } else {
                        *challenge_thread.lock().unwrap() = prompt;
                        "no\n".to_string()
                    }
                } else if regex::Regex::new(r"(?i)password|passphrase").unwrap().is_match(&prompt) {
                    format!("{password}\n")
                } else {
                    "\n".to_string()
                };
                let _ = stream.write_all(reply.as_bytes());
            }
        }
    });

    let exe = std::env::current_exe().map_err(|e| AppError::msg(e.to_string()))?;
    let helper = write_askpass_helper(&exe)?;
    for value in args.iter_mut() {
        if value == "BatchMode=yes" { *value = "BatchMode=no".into(); }
        if value == "StrictHostKeyChecking=yes" { *value = "StrictHostKeyChecking=ask".into(); }
    }
    let output = Command::new("ssh")
        .args(["-M", "-N", "-f", "-o", "ControlPersist=8h", "-o", "NumberOfPasswordPrompts=1"])
        .args(&args)
        .env("SSH_ASKPASS", &helper)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("DISPLAY", ":0")
        .env("CUE_ASK_SOCKET", format!("127.0.0.1:{port}"))
        .output()
        .await
        .map_err(|e| classify_ssh(&e.to_string()))?;
    let prompt = challenge.lock().unwrap().clone();
    if !prompt.is_empty() {
        return Err(AppError::Machine { code: "HOST_TRUST_REQUIRED".into(), prompt: Some(prompt) });
    }
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(classify_ssh(&message));
    }
    Ok(())
}

pub fn classify_ssh(message: &str) -> AppError {
    let code = if regex::Regex::new(r"(?i)too long for Unix domain socket|ENAMETOOLONG|unix_listener").unwrap().is_match(message) {
        "SOCKET_PATH"
    } else if regex::Regex::new(r"(?i)Host key verification failed|REMOTE HOST IDENTIFICATION HAS CHANGED").unwrap().is_match(message) {
        "HOST_KEY"
    } else if regex::Regex::new(r"(?i)Permission denied|incorrect passphrase|sign_and_send_pubkey").unwrap().is_match(message) {
        "AUTH_REQUIRED"
    } else if regex::Regex::new(r"(?i)timed out|ETIMEDOUT").unwrap().is_match(message) {
        "TIMEOUT"
    } else if regex::Regex::new(r"(?i)Connection refused").unwrap().is_match(message) {
        "REFUSED"
    } else if regex::Regex::new(r"(?i)Could not resolve hostname").unwrap().is_match(message) {
        "HOST_NOT_FOUND"
    } else if regex::Regex::new(r"(?i)can't cd|cannot cd|No such file or directory|Not a directory").unwrap().is_match(message) {
        "DIRECTORY"
    } else {
        "CONNECTION_FAILED"
    };
    AppError::machine(code)
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'"'"'"#))
}

pub fn ssh_login_command(command: &str) -> String {
    let run = format!("exec /bin/sh -c {}", shell_quote(command));
    format!("/bin/sh -c {}", shell_quote(&format!("exec \"${{SHELL:-/bin/sh}}\" -ilc {}", shell_quote(&run))))
}

pub async fn ssh_exec(host: &str, command: &str) -> AppResult<Vec<u8>> {
    ssh_exec_stdin(host, command, &[]).await
}

pub async fn ssh_exec_stdin(host: &str, command: &str, stdin: &[u8]) -> AppResult<Vec<u8>> {
    use tokio::io::AsyncWriteExt;
    let args = connection_args(host, false).await?;
    let mut child = Command::new("ssh")
        .args(args)
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| classify_ssh(&e.to_string()))?;
    if let Some(mut input) = child.stdin.take() {
        input.write_all(stdin).await.map_err(|e| classify_ssh(&e.to_string()))?;
    }
    let output = child.wait_with_output().await.map_err(|e| classify_ssh(&e.to_string()))?;
    if !output.status.success() {
        return Err(classify_ssh(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output.stdout)
}

pub async fn ssh_login_exec(host: &str, command: &str) -> AppResult<Vec<u8>> {
    let marker = format!("__CUE_LOGIN_{}__", uuid::Uuid::new_v4());
    let wrapped = ssh_login_command(&format!("printf '%s' {}; {}", shell_quote(&marker), command));
    let output = ssh_exec(host, &wrapped).await?;
    let start = output.windows(marker.len()).position(|w| w == marker.as_bytes()).ok_or_else(|| AppError::msg("远程 Shell 未执行检测命令，请检查 Shell 启动配置"))?;
    Ok(output[start + marker.len()..].to_vec())
}

/// SSH_ASKPASS must be a helper, not the GUI. Write a tiny wrapper next to the socket.
pub fn write_askpass_helper(exe: &std::path::Path) -> AppResult<PathBuf> {
    let dir = tempfile::Builder::new().prefix("cq-ask-").tempdir().map_err(|e| AppError::msg(e.to_string()))?;
    let path = dir.path().join(if cfg!(windows) { "askpass.cmd" } else { "askpass" });
    let script = if cfg!(windows) {
        format!("@echo off\n\"{}\" --askpass %*\n", exe.display())
    } else {
        format!("#!/bin/sh\nexec {} --askpass \"$1\"\n", shell_quote(&exe.to_string_lossy()))
    };
    std::fs::write(&path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    }
    let kept = path.clone();
    std::mem::forget(dir);
    Ok(kept)
}
