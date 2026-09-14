use crate::error::{AppError, AppResult};

pub struct Adapter {
    pub id: &'static str,
    pub executable: &'static str,
    pub args: &'static [&'static str],
}

impl Adapter {
    pub fn resume_args(&self, session_id: &str) -> AppResult<Vec<String>> {
        if !regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$").unwrap().is_match(session_id) {
            return Err(AppError::msg("无效的会话 ID，无法续接"));
        }
        Ok(match self.id {
            "pi" | "opencode" => vec!["--session".into(), session_id.into()],
            "antigravity" => vec!["--conversation".into(), session_id.into()],
            "codex" => {
                if !regex::Regex::new(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$").unwrap().is_match(session_id) {
                    return Err(AppError::msg("无效的 Codex 会话 ID，无法续接"));
                }
                vec!["resume".into(), session_id.into()]
            }
            _ => vec!["--resume".into(), session_id.into()],
        })
    }
}

pub fn adapter(id: &str) -> AppResult<Adapter> {
    Ok(match id {
        "codex" => Adapter {
            id: "codex",
            executable: "codex",
            args: &[
                "-c", r#"tui.terminal_title=["app-name","status","spinner","session-id"]"#,
                "-c", r#"tui.notifications=["plan-mode-prompt","approval-requested"]"#,
                "-c", r#"tui.notification_method="osc9""#,
                "-c", r#"tui.notification_condition="always""#,
            ],
        },
        "claude" => Adapter { id: "claude", executable: "claude", args: &[] },
        "cursor" => Adapter { id: "cursor", executable: "cursor-agent", args: &[] },
        "pi" => Adapter { id: "pi", executable: "pi", args: &[] },
        "omp" => Adapter { id: "omp", executable: "omp", args: &["--allow-home"] },
        "grok" => Adapter { id: "grok", executable: "grok", args: &[] },
        "gemini" => Adapter { id: "gemini", executable: "gemini", args: &[] },
        "opencode" => Adapter { id: "opencode", executable: "opencode", args: &[] },
        "antigravity" => Adapter { id: "antigravity", executable: "agy", args: &[] },
        "shell" => Adapter { id: "shell", executable: "", args: &[] },
        _ => return Err(AppError::msg("不支持的 CLI agent")),
    })
}
