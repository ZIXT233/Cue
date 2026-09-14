use crate::error::{AppError, AppResult};
use crate::models::QueueWorkspace;
use crate::ssh::{shell_quote, ssh_exec};
use base64::Engine;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub async fn inherited_config(kind: &str, workspace: &QueueWorkspace, node: &str) -> AppResult<Map<String, Value>> {
    let text = if workspace.kind == "ssh" {
        let host = workspace.ssh_host.as_deref().ok_or_else(|| AppError::msg("工作区不存在"))?;
        let script = if kind == "opencode" {
            r#"process.stdout.write(process.env.OPENCODE_CONFIG_CONTENT||"{}");"#
        } else {
            r#"const fs=require("node:fs"),p=require("node:path");const f=process.env.GEMINI_CLI_SYSTEM_DEFAULTS_PATH||(process.platform==="darwin"?"/Library/Application Support/GeminiCli/system-defaults.json":"/etc/gemini-cli/system-defaults.json");try{process.stdout.write(fs.readFileSync(f,"utf8"));}catch(e){if(e.code!=="ENOENT")throw e;process.stdout.write("{}");}"#
        };
        String::from_utf8_lossy(&ssh_exec(host, &[shell_quote(node), "-e".into(), shell_quote(script)].join(" ")).await?).into_owned()
    } else if kind == "opencode" {
        std::env::var("OPENCODE_CONFIG_CONTENT").unwrap_or_else(|_| "{}".into())
    } else {
        match std::fs::read_to_string(gemini_defaults_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".into(),
            Err(error) => return Err(error.into()),
        }
    };
    let text = text.trim().if_empty("{}");
    let config: Value = serde_json::from_str(&text)?;
    match config {
        Value::Object(map) => Ok(map),
        _ => Err(AppError::msg("无法读取现有 CLI 配置，未覆盖配置")),
    }
}

fn gemini_defaults_path() -> PathBuf {
    if let Ok(path) = std::env::var("GEMINI_CLI_SYSTEM_DEFAULTS_PATH") {
        return PathBuf::from(path);
    }
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/GeminiCli/system-defaults.json")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()))
            .join("gemini-cli")
            .join("system-defaults.json")
    } else {
        PathBuf::from("/etc/gemini-cli/system-defaults.json")
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for &str {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() { fallback.to_string() } else { self.to_string() }
    }
}

fn cursor_hook_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"[\\/](?:harness-plugins[\\/]cursor|\.cache[\\/]topcard[\\/]harness)[\\/].*hook\.cjs"#).unwrap())
}

pub fn is_topcard_cursor_command(command: &str, hook_path: &str) -> bool {
    let pattern = cursor_hook_pattern();
    if command.contains(hook_path) || pattern.is_match(command) {
        return true;
    }
    let Some(encoded) = command.split_once("-EncodedCommand").and_then(|(_, rest)| rest.split_whitespace().next()) else {
        return false;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return false;
    };
    if bytes.len() % 2 != 0 {
        return false;
    }
    let units: Vec<u16> = bytes.chunks_exact(2).map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]])).collect();
    let script = String::from_utf16_lossy(&units);
    script.contains(hook_path) || pattern.is_match(&script)
}

pub fn merge_cursor_user_hooks(existing: Value, incoming: &Value, hook_path: &str) -> AppResult<Value> {
    if !existing.is_null() && (existing.is_array() || !existing.is_object()) {
        return Err(AppError::msg("Invalid Cursor hooks configuration"));
    }
    let mut current = existing.as_object().cloned().unwrap_or_default();
    let mut hooks = current
        .get("hooks")
        .filter(|v| v.is_object() && !v.is_array())
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if let Some(incoming_hooks) = incoming.get("hooks").and_then(|v| v.as_object()) {
        for (event, entries) in incoming_hooks {
            let previous = hooks.get(event).and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let kept: Vec<Value> = previous
                .into_iter()
                .filter(|entry| !is_topcard_cursor_command(entry.get("command").and_then(|c| c.as_str()).unwrap_or(""), hook_path))
                .collect();
            let extra = entries.as_array().cloned().unwrap_or_default();
            hooks.insert(event.clone(), Value::Array(kept.into_iter().chain(extra).collect()));
        }
    }
    current.insert("version".into(), json!(1));
    current.insert("hooks".into(), Value::Object(hooks));
    Ok(Value::Object(current))
}

pub fn antigravity_owned(existing: &Value, command_for: &dyn Fn(&str) -> String) -> bool {
    let Some(obj) = existing.as_object() else { return false };
    let definitions: Vec<_> = obj.iter().filter(|(key, _)| *key != "enabled").collect();
    !definitions.is_empty()
        && definitions.iter().all(|(event, entries)| {
            ["PreInvocation", "PostInvocation", "PreToolUse", "PostToolUse", "Stop"].contains(&event.as_str())
                && entries.as_array().is_some_and(|items| {
                    !items.is_empty()
                        && items.iter().all(|entry| {
                            let handlers = entry
                                .get("hooks")
                                .and_then(|v| v.as_array())
                                .cloned()
                                .unwrap_or_else(|| vec![entry.clone()]);
                            !handlers.is_empty()
                                && handlers.iter().all(|handler| {
                                    handler.get("command").and_then(|c| c.as_str()).is_some_and(|command| {
                                        command == command_for(event)
                                            || Regex::new(r#"[\\/]\.topcard[\\/]harness-plugins[\\/]antigravity[\\/]hook\.cjs["']"#)
                                                .unwrap()
                                                .is_match(command)
                                    })
                                })
                        })
                })
        })
}

pub fn grok_owned(path: &Path) -> AppResult<()> {
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            let existing: Value = serde_json::from_str(&raw)?;
            if existing.get("topcardManaged") != Some(&json!(true)) {
                return Err(AppError::msg("Grok hook 同名文件不属于 Cue，未覆盖"));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn antigravity_guard(path: &Path, command_for: &dyn Fn(&str) -> String) -> AppResult<Value> {
    let config = match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error.into()),
    };
    if !config.is_object() || config.is_array() {
        return Err(AppError::msg("Invalid Antigravity hooks configuration"));
    }
    if let Some(existing) = config.get("topcard-session-state") {
        if !antigravity_owned(existing, command_for) {
            return Err(AppError::msg("Antigravity hook 同名条目不属于 Cue，未覆盖"));
        }
    }
    Ok(config)
}

pub fn cursor_user_hooks_path() -> PathBuf {
    if let Ok(path) = std::env::var("TOPCARD_CURSOR_HOOKS") {
        return PathBuf::from(path);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cursor/hooks.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_merge_keeps_foreign_and_replaces_owned() {
        let hook = "/home/u/.cache/topcard/harness/newtoken/hook.cjs";
        let existing = json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [
                    { "command": "echo foreign" },
                    { "command": "/home/u/.cache/topcard/harness/oldtoken/hook.cjs" }
                ]
            }
        });
        let incoming = json!({
            "hooks": {
                "beforeSubmitPrompt": [{ "command": format!("TOPCARD_HARNESS_KIND=cursor /usr/bin/node {hook} beforeSubmitPrompt") }]
            }
        });
        let merged = merge_cursor_user_hooks(existing, &incoming, hook).unwrap();
        let commands: Vec<_> = merged["hooks"]["beforeSubmitPrompt"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["command"].as_str().unwrap())
            .collect();
        assert_eq!(commands, vec![
            "echo foreign",
            "TOPCARD_HARNESS_KIND=cursor /usr/bin/node /home/u/.cache/topcard/harness/newtoken/hook.cjs beforeSubmitPrompt",
        ]);
    }

    #[test]
    fn cursor_rejects_array_config() {
        let err = merge_cursor_user_hooks(json!([]), &json!({ "hooks": {} }), "/tmp/hook.cjs").unwrap_err();
        assert_eq!(err.to_string(), "Invalid Cursor hooks configuration");
    }

    #[test]
    fn grok_refuses_unowned_file() {
        let dir = std::env::temp_dir().join(format!("cue-grok-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("topcard-session-state.json");
        std::fs::write(&path, r#"{"hooks":{}}"#).unwrap();
        assert!(grok_owned(&path).is_err());
        std::fs::write(&path, r#"{"topcardManaged":true}"#).unwrap();
        assert!(grok_owned(&path).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }
}
