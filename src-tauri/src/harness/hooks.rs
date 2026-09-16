use crate::error::{AppError, AppResult};
use crate::harness::inherited::{
    antigravity_guard, cursor_user_hooks_path, grok_owned, inherited_config, merge_cursor_user_hooks,
};
use crate::models::QueueWorkspace;
use crate::paths::{atomic_write, plugin_root, signal_dir};
use crate::ssh::{shell_quote, ssh_exec, ssh_login_exec};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    let source = std::fs::read_to_string(bin_dir.join("harness-hook.cjs"))?;
    let mut root = plugin_root(kind);
    if workspace.kind == "local" {
        std::fs::create_dir_all(directory)?;
    }
    let mut node = which::which("node").map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "node".into());
    let mut extra_env = HashMap::new();
    let mut grok_config_path = None;
    let mut antigravity_config_path = None;
    let mut cursor_config_path = None;
    if workspace.kind == "ssh" {
        let host = workspace.ssh_host.as_deref().ok_or_else(|| AppError::msg("工作区不存在"))?;
        let home = String::from_utf8_lossy(&ssh_exec(host, r#"printf "%s" "$HOME""#).await?).trim().to_string();
        if !home.starts_with('/') {
            return Err(AppError::msg("无法确定远程主机的 Home 目录"));
        }
        let digest = hex::encode(&Sha256::digest(source.as_bytes())[..8]);
        root = PathBuf::from(match kind {
            "grok" => format!("{home}/.cache/cue/harness-plugins/grok"),
            "codex" => format!("{home}/.cache/cue/harness-plugins/codex/{digest}"),
            "cursor" => format!("{home}/.cache/cue/harness-plugins/cursor"),
            _ => format!("{home}/.cache/cue/harness/{token}"),
        });
        if kind == "cursor" {
            cursor_config_path = Some(format!("{home}/.cursor/hooks.json"));
        }
        node = String::from_utf8_lossy(&ssh_login_exec(host, "command -v node").await?).trim().to_string();
        if !node.starts_with('/') {
            return Err(AppError::msg("远程 Harness 状态探针需要 Node.js，请先在主机安装 Node.js"));
        }
    }
    let hook_path = if workspace.kind == "ssh" { format!("{}/hook.cjs", root.display()) } else { root.join("hook.cjs").to_string_lossy().into_owned() };
    let windows = workspace.kind == "local" && cfg!(windows);
    let hook_timeout = if windows { if kind == "cursor" { 15 } else { 5 } } else { 2 };
    let quote = |value: &str| {
        if workspace.kind == "local" && cfg!(windows) {
            format!("\"{}\"", value.replace('"', "\\\""))
        } else {
            shell_quote(value)
        }
    };
    let command_for = |event: Option<&str>| -> String {
        if windows && kind == "cursor" {
            return [node.as_str(), hook_path.as_str()].into_iter().chain(event).map(quote).collect::<Vec<_>>().join(" ");
        }
        if windows {
            return super::windows::windows_hook_command(&node, &hook_path, event, &[]);
        }
        // Bake kind so foreign IDE sessions still return the required JSON reply.
        let prefix = if kind == "cursor" { "CUE_HARNESS_KIND=cursor " } else { "" };
        format!("{prefix}{}{}", [node.as_str(), hook_path.as_str()].into_iter().map(quote).collect::<Vec<_>>().join(" "), event.map(|e| format!(" {}", quote(e))).unwrap_or_default())
    };
    let command = command_for(None);
    let mut files = HashMap::from([("hook.cjs".to_string(), source)]);
    let mut args = Vec::new();
    if kind == "antigravity" {
        let events = ["PreInvocation", "PostInvocation", "PreToolUse", "PostToolUse", "Stop"];
        let mut bundle = serde_json::Map::new();
        for event in events {
            let hook = serde_json::json!({ "type": "command", "command": command_for(Some(event)), "timeout": hook_timeout });
            bundle.insert(event.into(), if event.ends_with("ToolUse") {
                serde_json::json!([{ "matcher": "*", "hooks": [hook] }])
            } else {
                serde_json::json!([hook])
            });
        }
        files.insert("antigravity-hooks.json".into(), serde_json::Value::Object(bundle).to_string());
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        antigravity_config_path = Some(if workspace.kind == "ssh" {
            let host = workspace.ssh_host.as_deref().unwrap();
            format!("{}/.gemini/config/hooks.json", String::from_utf8_lossy(&ssh_exec(host, r#"printf "%s" "$HOME""#).await?).trim())
        } else {
            home.join(".gemini/config/hooks.json").to_string_lossy().into_owned()
        });
    } else if kind == "gemini" {
        let mut config = inherited_config("gemini", workspace, &node).await?;
        let mut hooks = config.get("hooks").and_then(|v| v.as_object()).cloned().unwrap_or_default();
        let gemini_timeout = hook_timeout * 1000;
        for event in ["SessionStart", "BeforeAgent", "AfterAgent", "BeforeTool", "AfterTool", "Notification"] {
            let mut entries = hooks.get(event).and_then(|v| v.as_array()).cloned().unwrap_or_default();
            entries.push(serde_json::json!({
                "hooks": [{ "type": "command", "name": format!("cue-{event}"), "command": command, "timeout": gemini_timeout }]
            }));
            hooks.insert(event.into(), serde_json::Value::Array(entries));
        }
        config.insert("hooks".into(), serde_json::Value::Object(hooks));
        files.insert("system-defaults.json".into(), serde_json::Value::Object(config).to_string());
        extra_env.insert("GEMINI_CLI_SYSTEM_DEFAULTS_PATH".into(), if workspace.kind == "ssh" {
            format!("{}/system-defaults.json", root.display())
        } else {
            root.join("system-defaults.json").to_string_lossy().into_owned()
        });
    } else if kind == "opencode" {
        files.insert("opencode-plugin.mjs".into(), std::fs::read_to_string(bin_dir.join("harness-opencode.mjs"))?);
        let mut config = inherited_config("opencode", workspace, &node).await?;
        let plugin = if workspace.kind == "ssh" {
            format!("file://{}/opencode-plugin.mjs", root.to_string_lossy().split('/').map(|s| urlencoding_lite(s)).collect::<Vec<_>>().join("/"))
        } else {
            url::Url::from_file_path(root.join("opencode-plugin.mjs")).map(|u| u.to_string()).unwrap_or_default()
        };
        let mut plugins = config.get("plugin").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        plugins.push(serde_json::Value::String(plugin));
        config.insert("plugin".into(), serde_json::Value::Array(plugins));
        extra_env.insert("OPENCODE_CONFIG_CONTENT".into(), serde_json::Value::Object(config).to_string());
    } else if kind == "grok" {
        let events = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "Stop", "StopFailure", "StopCancelled", "Notification"];
        let mut hooks = serde_json::Map::new();
        for event in events {
            hooks.insert(event.into(), serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": hook_timeout }] }]));
        }
        files.insert("grok-hooks.json".into(), serde_json::json!({ "cueManaged": true, "hooks": hooks }).to_string());
        grok_config_path = Some(if workspace.kind == "ssh" {
            let host = workspace.ssh_host.as_deref().unwrap();
            format!("{}/hooks/cue-session-state.json", String::from_utf8_lossy(&ssh_exec(host, r#"printf "%s" "${GROK_HOME:-$HOME/.grok}""#).await?).trim())
        } else {
            std::env::var("GROK_HOME").map(PathBuf::from).unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".grok"))
                .join("hooks/cue-session-state.json").to_string_lossy().into_owned()
        });
    } else if kind == "pi" || kind == "omp" {
        files.insert("pi-extension.mjs".into(), std::fs::read_to_string(bin_dir.join("harness-pi.mjs"))?);
        args.extend(["--extension".into(), if workspace.kind == "ssh" { format!("{}/pi-extension.mjs", root.display()) } else { root.join("pi-extension.mjs").to_string_lossy().into_owned() }]);
    } else if kind == "codex" {
        args.extend(["--enable".into(), "hooks".into()]);
        for event in ["SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse", "Stop"] {
            args.extend(["-c".into(), format!("hooks.{event}=[{{hooks=[{{type=\"command\",command={},timeout={hook_timeout}}}]}}]", serde_json::to_string(&command).unwrap())]);
        }
    } else {
        let cursor = kind == "cursor";
        // CodeBuddy keeps the Claude plugin layout but resolves its own manifest
        // directory first (`.codebuddy-plugin`); hooks/hooks.json is read the same way.
        let manifest_dir = if cursor { ".cursor-plugin" } else if kind == "codebuddy" { ".codebuddy-plugin" } else { ".claude-plugin" };
        files.insert(
            format!("{manifest_dir}/plugin.json"),
            serde_json::json!({ "name": "cue-session-state", "version": "1.0.0", "description": "Report this Cue terminal's lifecycle" }).to_string(),
        );
        let events: &[&str] = if cursor {
            &["sessionStart", "beforeSubmitPrompt", "preToolUse", "postToolUse", "postToolUseFailure", "beforeShellExecution", "beforeMCPExecution", "afterAgentResponse", "stop", "sessionEnd"]
        } else {
            // Notification is what a blocked prompt actually reaches us through:
            // permission prompts and ask-style tools never surface as a tool call,
            // so without it a card waits forever with no signal to react to.
            &["SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "Notification", "PostToolUse", "PostToolUseFailure", "Stop", "StopFailure"]
        };
        let mut hooks = serde_json::Map::new();
        for event in events {
            hooks.insert(event.to_string(), if cursor {
                serde_json::json!([{ "command": command_for(Some(event)), "timeout": hook_timeout }])
            } else {
                serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": hook_timeout }] }])
            });
        }
        files.insert("hooks/hooks.json".into(), if cursor {
            serde_json::json!({ "version": 1, "hooks": {} }).to_string()
        } else {
            serde_json::json!({ "hooks": hooks }).to_string()
        });
        if cursor {
            files.insert("cursor-user-hooks.json".into(), serde_json::json!({ "version": 1, "hooks": hooks }).to_string());
            if workspace.kind == "local" {
                cursor_config_path = Some(cursor_user_hooks_path().to_string_lossy().into_owned());
            } else {
                files.insert(format!("cards/{token}/.keep"), String::new());
            }
        }
        args.extend(["--plugin-dir".into(), root.to_string_lossy().into_owned()]);
    }

    if workspace.kind == "ssh" {
        let host = workspace.ssh_host.as_deref().unwrap();
        let payload = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, serde_json::to_string(&files)?);
        let installer = r#"const fs=require("node:fs"),p=require("node:path"),root=process.argv[1];for(const [name,body] of Object.entries(JSON.parse(Buffer.from(process.argv[2],"base64")))){const f=p.join(root,name);fs.mkdirSync(p.dirname(f),{recursive:true,mode:448});const tmp=f+"."+require("node:crypto").randomUUID()+".tmp";fs.writeFileSync(tmp,body,{mode:384});fs.renameSync(tmp,f);}"#;
        ssh_exec(host, &[shell_quote(&node), "-e".into(), shell_quote(installer), shell_quote(&root.to_string_lossy()), shell_quote(&payload)].join(" ")).await?;
        if let Some(path) = grok_config_path {
            let install = r#"const fs=require("node:fs"),p=require("node:path"),src=process.argv[1],dest=process.argv[2];if(fs.existsSync(dest)&&JSON.parse(fs.readFileSync(dest,"utf8")).cueManaged!==true)throw Error("Existing hook file is not owned by Cue");fs.mkdirSync(p.dirname(dest),{recursive:true,mode:448});fs.copyFileSync(src,dest);fs.chmodSync(dest,384);"#;
            ssh_exec(host, &[shell_quote(&node), "-e".into(), shell_quote(install), shell_quote(&format!("{}/grok-hooks.json", root.display())), shell_quote(&path)].join(" ")).await?;
        }
        if let Some(path) = antigravity_config_path {
            let installer = r#"const fs=require("node:fs"),p=require("node:path"),dest=process.argv[1],src=process.argv[2];const x=fs.existsSync(dest)?JSON.parse(fs.readFileSync(dest,"utf8")):{};if(x["cue-session-state"]&&!JSON.stringify(x["cue-session-state"]).includes("/cue/"))throw Error("Hook name already owned");x["cue-session-state"]=JSON.parse(fs.readFileSync(src,"utf8"));fs.mkdirSync(p.dirname(dest),{recursive:true});fs.writeFileSync(dest+".cue.tmp",JSON.stringify(x,null,2),{mode:384});fs.renameSync(dest+".cue.tmp",dest);"#;
            ssh_exec(host, &[shell_quote(&node), "-e".into(), shell_quote(installer), shell_quote(&path), shell_quote(&format!("{}/antigravity-hooks.json", root.display()))].join(" ")).await?;
        }
        if let Some(path) = cursor_config_path {
            let installer = r#"const fs=require("node:fs"),p=require("node:path"),dest=process.argv[1],src=process.argv[2],hook=process.argv[3];const owned=c=>{if(typeof c!=="string")return false;if(c.includes(hook))return true;const m=c.match(/-EncodedCommand\s+(\S+)/);if(m){try{const s=Buffer.from(m[1],"base64").toString("utf16le");if(s.includes(hook)||/[\\/](?:harness-plugins[\\/]cursor|\.cache[\\/]cue[\\/]harness)[\\/].*hook\.cjs/.test(s))return true;}catch{}}return /[\\/](?:harness-plugins[\\/]cursor|\.cache[\\/]cue[\\/]harness)[\\/].*hook\.cjs/.test(c)};const incoming=JSON.parse(fs.readFileSync(src,"utf8"));let x=fs.existsSync(dest)?JSON.parse(fs.readFileSync(dest,"utf8")):{};if(!x||Array.isArray(x)||typeof x!=="object")throw Error("Invalid Cursor hooks configuration");const hooks={...(x.hooks&&typeof x.hooks==="object"&&!Array.isArray(x.hooks)?x.hooks:{})};for(const [event,entries] of Object.entries(incoming.hooks||{})){const cur=Array.isArray(hooks[event])?hooks[event]:[];hooks[event]=[...cur.filter(e=>!owned(e&&e.command)),...entries];}x={...x,version:1,hooks};fs.mkdirSync(p.dirname(dest),{recursive:true,mode:448});fs.writeFileSync(dest+".cue.tmp",JSON.stringify(x,null,2),{mode:384});fs.renameSync(dest+".cue.tmp",dest);"#;
            ssh_exec(host, &[shell_quote(&node), "-e".into(), shell_quote(installer), shell_quote(&path), shell_quote(&format!("{}/cursor-user-hooks.json", root.display())), shell_quote(&hook_path)].join(" ")).await?;
        }
        extra_env.insert("CUE_HARNESS_CHANNEL".into(), token.into());
        extra_env.insert("CUE_HARNESS_KIND".into(), kind.into());
        // Fire the hook's internal watchdog before the runner's kill deadline.
        extra_env.insert("CUE_HARNESS_WATCHDOG_MS".into(), ((hook_timeout - 2).max(1) * 1000).to_string());
        if crate::debuglog::verbose() {
            extra_env.insert("CUE_HARNESS_DEBUG".into(), "1".into());
        }
        if kind == "cursor" {
            extra_env.insert("CUE_HARNESS_SIGNAL_DIR".into(), format!("{}/cards/{token}", root.display()));
        }
        return Ok(HookLaunch { args, env: extra_env });
    }

    for (name, body) in &files {
        let path = root.join(name);
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        atomic_write(&path, body)?;
    }
    if let Some(path) = grok_config_path {
        grok_owned(Path::new(&path))?;
        if let Some(parent) = Path::new(&path).parent() { std::fs::create_dir_all(parent)?; }
        atomic_write(Path::new(&path), files.get("grok-hooks.json").map(String::as_str).unwrap_or(""))?;
    }
    if let Some(path) = antigravity_config_path {
        let mut config = antigravity_guard(Path::new(&path), &|event: &str| command_for(Some(event)))?;
        if let Some(obj) = config.as_object_mut() {
            obj.insert("cue-session-state".into(), serde_json::from_str(files.get("antigravity-hooks.json").map(String::as_str).unwrap_or("{}"))?);
        }
        if let Some(parent) = Path::new(&path).parent() { std::fs::create_dir_all(parent)?; }
        atomic_write(Path::new(&path), &serde_json::to_string_pretty(&config)?)?;
    }
    if let Some(path) = cursor_config_path {
        if let Some(incoming) = files.get("cursor-user-hooks.json") {
            let existing: serde_json::Value = match std::fs::read_to_string(&path) {
                Ok(raw) => serde_json::from_str(&raw)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
                Err(error) => return Err(error.into()),
            };
            let merged = merge_cursor_user_hooks(existing, &serde_json::from_str(incoming)?, &hook_path)?;
            if let Some(parent) = Path::new(&path).parent() { std::fs::create_dir_all(parent)?; }
            atomic_write(Path::new(&path), &serde_json::to_string_pretty(&merged)?)?;
        }
    }
    extra_env.insert("CUE_HARNESS_SIGNAL_DIR".into(), directory.to_string_lossy().into_owned());
    extra_env.insert("CUE_HARNESS_KIND".into(), kind.into());
    // Fire the hook's internal watchdog before the runner's kill deadline.
    extra_env.insert("CUE_HARNESS_WATCHDOG_MS".into(), ((hook_timeout - 2).max(1) * 1000).to_string());
    if crate::debuglog::verbose() {
        extra_env.insert("CUE_HARNESS_DEBUG".into(), "1".into());
    }
    let _ = signal_dir(token);
    Ok(HookLaunch { args, env: extra_env })
}

fn urlencoding_lite(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
