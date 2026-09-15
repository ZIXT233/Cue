use crate::error::AppResult;
use crate::winproc::NoWindow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const START: &str = "__CUE_ENV_START__";
const END: &str = "__CUE_ENV_END__";

pub async fn local_environment(force: bool) -> AppResult<HashMap<String, String>> {
    let _ = force;
    let output = if cfg!(windows) {
        let command = format!(
            "$e=@{{}};[Environment]::GetEnvironmentVariables().GetEnumerator()|ForEach-Object{{$e[$_.Key]=[string]$_.Value}};[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false);[Console]::Write('{START}');[Console]::Write(($e|ConvertTo-Json -Compress));[Console]::Write('{END}')"
        );
        tokio::process::Command::new("powershell.exe").args(["-NoLogo", "-Command", &command]).no_window().output().await
    } else {
        let command = format!("printf '{START}\\0'; /usr/bin/env -0; printf '{END}\\0'");
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        tokio::process::Command::new(shell).args(["-ilc", &command]).output().await
    }
    .map_err(|e| crate::error::AppError::msg(format!("读取用户 Shell 环境失败：{e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let start = stdout.find(START).ok_or_else(|| crate::error::AppError::msg("未能读取 Shell 环境，请检查 Shell 配置中的启动命令"))?;
    let end = stdout[start + START.len()..].find(END).ok_or_else(|| crate::error::AppError::msg("未能读取 Shell 环境，请检查 Shell 配置中的启动命令"))?;
    let body = &stdout[start + START.len()..start + START.len() + end];
    let mut env: HashMap<String, String> = std::env::vars().collect();
    if cfg!(windows) {
        if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(body) {
            for (key, value) in parsed {
                env.retain(|k, _| !k.eq_ignore_ascii_case(&key));
                env.insert(key, value);
            }
        }
    } else {
        for item in body.split('\0').filter(|s| s.contains('=')) {
            if let Some((key, value)) = item.split_once('=') {
                env.insert(key.to_string(), value.to_string());
            }
        }
    }
    Ok(env)
}

pub fn resolve_local_command(command: &str, env: &HashMap<String, String>) -> Option<String> {
    let read = |key: &str| env.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone());
    let path = read("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ';' } else { ':' };
    let mut dirs: Vec<PathBuf> = path.split(sep).filter(|s| Path::new(s).is_absolute()).map(PathBuf::from).collect();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".opencode/bin"));
    }
    if cfg!(windows) {
        if let Some(appdata) = read("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
    }
    let extensions: Vec<String> = if cfg!(windows) {
        read("PATHEXT").unwrap_or_else(|| ".EXE;.CMD;.BAT;.COM".into()).split(';').map(|s| s.to_string()).collect()
    } else {
        vec![String::new()]
    };
    for dir in dirs {
        for ext in &extensions {
            let candidate = dir.join(format!("{command}{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}
