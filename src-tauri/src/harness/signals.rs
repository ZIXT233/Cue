use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HookSignal {
    pub kind: Option<String>,
    pub at: i64,
    pub event: String,
    pub reply_preview: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub tool: Option<String>,
    pub prompt: Option<String>,
    pub first_prompt: Option<String>,
    pub title: Option<String>,
    pub notification: Option<String>,
    /// Cold-start workspace reported by a global (non-Cue) hook.
    pub workspace_root: Option<String>,
    /// Set by the ingress when the emitting process carried no Cue channel: the event
    /// belongs to an external session, not to a queue card.
    pub external: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct ProbeState {
    pub reply_preview: Option<String>,
    pub antigravity_completed: bool,
    pub state: String,
    pub at: i64,
    pub shell_command_started_at: Option<i64>,
    pub shell_command_running: Option<bool>,
    pub shell_exit_code: Option<i32>,
    pub kind: Option<String>,
    pub remote: bool,
    pub session_name: Option<String>,
    /// First user prompt from the session file, when that file can be read.
    pub first_prompt: Option<String>,
    /// Last hooked submit on this card. Updates on each submit event.
    pub submit_prompt: Option<String>,
    pub title_state: Option<String>,
    pub title_seen: bool,
    pub hook_seen: bool,
    pub notify_osc_seen: Vec<String>,
    pub notify_osc_hits: u32,
    pub session_id: Option<String>,
    pub identity_at: Option<i64>,
    pub session_id_prefix: Option<String>,
    pub source: Option<String>,
    /// Wall-clock ms when this terminal's *first* hook signal was ingested.
    ///
    /// Together with the PTY probe's `spawned_at_ms` this splits a slow card in
    /// two: `first_hook - spawn` is the second leg (how long the CLI took to
    /// reach its first hook), and comparing it against the CLI's own
    /// `first_byte_ms` tells us whether the cost sat in bootstrap or after it.
    /// First write wins; later hooks never move it.
    pub first_hook_at_ms: Option<i64>,
    /// The terminal's spawn instant, copied in at launch. Kept here (not only on
    /// the PTY probe) so the signal path can compute its own deltas without
    /// reaching across into the terminal module's lock.
    pub spawned_at_ms: Option<i64>,
}

pub fn normalize_event(event: &str) -> &str {
    match event {
        "sessionStart" | "SessionStart" => "SessionStart",
        "beforeSubmitPrompt" | "UserPromptSubmit" | "BeforeAgent" | "PreInvocation"
        | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "BeforeTool" | "AfterTool"
        | "PostInvocation" | "preToolUse" | "postToolUse" | "postToolUseFailure" => "UserPromptSubmit",
        "stop" | "Stop" | "StopFailure" | "StopCancelled" | "sessionEnd" | "AfterAgent" | "afterAgentResponse" => "Stop",
        "beforeShellExecution" | "beforeMCPExecution" | "PermissionRequest" => "PermissionRequest",
        "SessionInfo" => "SessionInfo",
        other => other,
    }
}

pub fn hook_state(signal: &HookSignal) -> Option<&'static str> {
    if signal.agent_id.is_some() { return None; }
    let event = normalize_event(&signal.event);
    let notification = signal.notification.as_deref().unwrap_or("");
    if (signal.event == "Notification" || event == "Notification") && ["permission_prompt", "ToolPermission", "idle_prompt"].contains(&notification) {
        return Some("attention");
    }
    if event == "PermissionRequest" { return Some("attention"); }
    if signal.kind.as_deref() == Some("antigravity") && signal.event == "PreToolUse" {
        return Some("attention");
    }
    // Cursor gates tool execution behind a permission prompt; preToolUse fires right
    // before it for every tool (matcher "*"), so the whole event is the ask moment.
    if signal.kind.as_deref() == Some("cursor") && signal.event == "preToolUse" {
        return Some("attention");
    }
    if event == "UserPromptSubmit" { return Some("working"); }
    if event == "Stop" { return Some("attention"); }
    None
}

pub fn observe_hook(current: ProbeState, raw: HookSignal) -> ProbeState {
    if raw.agent_id.is_some() { return current; }
    let event = normalize_event(&raw.event);
    let session_id = raw.session_id.as_deref().filter(|id| regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$").unwrap().is_match(id)).map(|s| s.to_string());
    // Signals already landed on this card (process env). Cursor /resume and TUI
    // session switches often skip sessionStart and reuse a different conversation id.
    let clean = |value: Option<&String>| value.map(|s| s.chars().filter(|c| !c.is_control()).take(160).collect::<String>()).filter(|s| !s.trim().is_empty());
    let new_identity = session_id.as_ref().is_some_and(|id| current.session_id.as_ref() != Some(id));
    let submit_event = ["beforeSubmitPrompt", "UserPromptSubmit", "BeforeAgent"].contains(&raw.event.as_str());
    let mut next = current.clone();
    next.hook_seen = true;
    if new_identity {
        next.session_name = None;
        next.first_prompt = None;
        next.submit_prompt = None;
    }
    // OpenCode: stable OSC title. Others: hook title is a live hint; local file refresh overwrites.
    if let Some(name) = clean(raw.title.as_ref()) {
        next.session_name = Some(name);
    }
    if let Some(first) = clean(raw.first_prompt.as_ref()) {
        next.first_prompt = Some(first);
    }
    if submit_event {
        if let Some(prompt) = clean(raw.prompt.as_ref()) {
            next.submit_prompt = Some(prompt);
        }
    }
    if let Some(id) = session_id {
        if raw.at >= current.identity_at.unwrap_or(0) {
            next.session_id = Some(id);
            next.session_id_prefix = None;
            next.identity_at = Some(raw.at);
        }
    }
    if raw.at < current.at { return next; }
    if raw.kind.as_deref() == Some("antigravity") && current.antigravity_completed && !new_identity && raw.event != "PreInvocation" && event != "Stop" {
        return next;
    }
    if raw.kind.as_deref() == Some("antigravity") {
        next.antigravity_completed = event == "Stop";
    }
    if let Some(state) = hook_state(&raw) {
        next.reply_preview = if state == "working" { None } else { clean(raw.reply_preview.as_ref()).or_else(|| if new_identity { None } else { current.reply_preview.clone() }) };
        next.state = state.into();
        next.at = raw.at;
        next.source = Some("hook".into());
    } else if event == "SessionStart" && current.state == "starting" {
        next.state = "attention".into();
    }
    next
}

/// Last writer wins: any source that reports a state change updates the card.
/// A missed transition (hook lost to a timeout, CLI aborting a turn without a
/// Stop) leaves the card stuck far longer than a flapped state does, so the
/// title probe is never demoted — even when hooks have been seen.
pub fn observe_title(current: ProbeState, state: &str, at: i64) -> ProbeState {
    let mut next = current;
    if next.title_state.as_deref() == Some(state) {
        next.title_seen = true;
        return next;
    }
    next.title_seen = true;
    next.title_state = Some(state.into());
    next.state = state.into();
    next.at = at;
    next.source = Some("title".into());
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude-family `PreToolUse` means "the tool is running", so a CodeBuddy tool
    /// call stays "working"; only a bare turn end, or a prompt notification, waits
    /// on the user. Notification is what a blocked ask actually arrives through.
    #[test]
    fn codebuddy_tools_are_working_and_prompts_are_attention() {
        let edit = HookSignal {
            kind: Some("codebuddy".into()),
            event: "PreToolUse".into(),
            tool: Some("Explain".into()),
            at: 1,
            ..HookSignal::default()
        };
        assert_eq!(hook_state(&edit), Some("working"));

        let stop = HookSignal {
            kind: Some("codebuddy".into()),
            event: "Stop".into(),
            at: 2,
            ..HookSignal::default()
        };
        assert_eq!(hook_state(&stop), Some("attention"));

        let blocked = HookSignal {
            kind: Some("codebuddy".into()),
            event: "Notification".into(),
            notification: Some("permission_prompt".into()),
            at: 3,
            ..HookSignal::default()
        };
        assert_eq!(hook_state(&blocked), Some("attention"));
    }

    #[test]
    fn last_submit_prompt_wins() {
        let started = observe_hook(ProbeState::default(), HookSignal {
            event: "sessionStart".into(),
            at: 1,
            session_id: Some("s1".into()),
            ..HookSignal::default()
        });
        let first = observe_hook(started, HookSignal {
            event: "beforeSubmitPrompt".into(),
            at: 2,
            session_id: Some("s1".into()),
            prompt: Some("first".into()),
            ..HookSignal::default()
        });
        let next = observe_hook(first, HookSignal {
            event: "beforeSubmitPrompt".into(),
            at: 3,
            session_id: Some("s1".into()),
            prompt: Some("second".into()),
            ..HookSignal::default()
        });
        assert_eq!(next.submit_prompt.as_deref(), Some("second"));
        assert_eq!(next.session_name, None);
    }

    #[test]
    fn hook_title_is_session_name() {
        let next = observe_hook(ProbeState::default(), HookSignal {
            event: "sessionStart".into(),
            at: 1,
            session_id: Some("s1".into()),
            title: Some("Ask Me".into()),
            prompt: Some("ignored".into()),
            ..HookSignal::default()
        });
        assert_eq!(next.session_name.as_deref(), Some("Ask Me"));
        assert_eq!(next.submit_prompt, None);
    }

    #[test]
    fn cursor_resume_prompt_switches_identity() {
        let resumed = ProbeState {
            kind: Some("cursor".into()),
            state: "starting".into(),
            session_id: Some("c66a63c4-1cf6-467a-92b9-ee46fc9c47d3".into()),
            session_name: Some("Hello There".into()),
            first_prompt: Some("/resume".into()),
            ..ProbeState::default()
        };
        let next = observe_hook(resumed, HookSignal {
            kind: Some("cursor".into()),
            event: "beforeSubmitPrompt".into(),
            at: 2,
            session_id: Some("c6553b99-eef0-4d2a-af62-8deaa625f841".into()),
            prompt: Some("你好".into()),
            ..HookSignal::default()
        });
        assert_eq!(next.state, "working");
        assert!(next.hook_seen);
        assert_eq!(next.session_id.as_deref(), Some("c6553b99-eef0-4d2a-af62-8deaa625f841"));
        assert_eq!(next.submit_prompt.as_deref(), Some("你好"));
    }

    #[test]
    fn cursor_prompt_binds_without_session_start() {
        let next = observe_hook(ProbeState {
            kind: Some("cursor".into()),
            state: "starting".into(),
            ..ProbeState::default()
        }, HookSignal {
            kind: Some("cursor".into()),
            event: "beforeSubmitPrompt".into(),
            at: 1,
            session_id: Some("c6553b99-eef0-4d2a-af62-8deaa625f841".into()),
            prompt: Some("你好".into()),
            ..HookSignal::default()
        });
        assert_eq!(next.state, "working");
        assert!(next.hook_seen);
        assert_eq!(next.session_id.as_deref(), Some("c6553b99-eef0-4d2a-af62-8deaa625f841"));
    }

    #[test]
    fn antigravity_pre_tool_use_is_attention() {
        let next = observe_hook(ProbeState {
            kind: Some("antigravity".into()),
            state: "working".into(),
            ..ProbeState::default()
        }, HookSignal {
            kind: Some("antigravity".into()),
            event: "PreToolUse".into(),
            at: 2,
            tool: Some("run_command".into()),
            ..HookSignal::default()
        });
        assert_eq!(next.state, "attention");
        let after = observe_hook(next, HookSignal {
            kind: Some("antigravity".into()),
            event: "PostToolUse".into(),
            at: 3,
            tool: Some("run_command".into()),
            ..HookSignal::default()
        });
        assert_eq!(after.state, "working");
    }

    #[test]
    fn cursor_pre_tool_use_is_attention() {
        let next = observe_hook(ProbeState {
            kind: Some("cursor".into()),
            state: "working".into(),
            ..ProbeState::default()
        }, HookSignal {
            kind: Some("cursor".into()),
            event: "preToolUse".into(),
            at: 2,
            tool: Some("Shell".into()),
            ..HookSignal::default()
        });
        assert_eq!(next.state, "attention");
        let after = observe_hook(next, HookSignal {
            kind: Some("cursor".into()),
            event: "postToolUse".into(),
            at: 3,
            tool: Some("Shell".into()),
            ..HookSignal::default()
        });
        assert_eq!(after.state, "working");
    }

    #[test]
    fn title_wins_even_after_hooks() {
        let current = ProbeState {
            state: "attention".into(),
            hook_seen: true,
            source: Some("hook".into()),
            ..ProbeState::default()
        };
        let next = observe_title(current, "working", 10);
        assert_eq!(next.state, "working");
        assert_eq!(next.title_state.as_deref(), Some("working"));
        assert_eq!(next.source.as_deref(), Some("title"));
    }

    #[test]
    fn title_exit_from_working_recovers_a_lost_stop_hook() {
        // The codex card stuck in "working": the Stop hook never arrived, only
        // the title probe saw the turn end. The title must still flip the card.
        let current = ProbeState {
            state: "working".into(),
            hook_seen: true,
            source: Some("hook".into()),
            ..ProbeState::default()
        };
        let next = observe_title(current, "attention", 10);
        assert_eq!(next.state, "attention");
    }
}
