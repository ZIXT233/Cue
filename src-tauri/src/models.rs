use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

fn i64_from_json(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().map(|n| n as i64)).or_else(|| value.as_f64().map(|n| n as i64))
}

fn deserialize_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    i64_from_json(&value).ok_or_else(|| serde::de::Error::custom("expected a number"))
}

fn deserialize_opt_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(Value::Null) => None,
        Some(value) => Some(i64_from_json(&value).unwrap_or(0)),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub path: String,
    pub id: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub created: String,
    pub modified: String,
    pub message_count: u64,
    pub first_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CardPhase {
    Draft,
    Working,
    Attention,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueWorkspace {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    pub runtime_cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_opt_i64")]
    pub default_conversation_weight: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessSession {
    pub kind: String,
    pub terminal_id: String,
    pub state: String,
    #[serde(default)]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_command_notifications: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_command_started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_command_running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_notify: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session_id: Option<String>,
    /// Real session name (session file, or OpenCode OSC title).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    /// First user prompt from the session file when the name is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_prompt: Option<String>,
    /// Last hooked submit on this card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unpersisted_session: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedLease {
    pub owner: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSideTerminal {
    pub id: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueCard {
    pub id: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub session: Option<SessionInfo>,
    pub phase: CardPhase,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_opt_i64")]
    pub priority_weight: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_key: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urgent_call: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_evaluation: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_history: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached: Option<DetachedLease>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_terminals: Option<Vec<CardSideTerminal>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_terminal_open: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<HarnessSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_sources: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnTag {
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_i64")]
    pub weight: i64,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardQueue {
    pub version: u32,
    pub revision: u64,
    pub cards: Vec<QueueCard>,
    pub order: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_tags_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_tag_definitions: Option<Vec<TurnTag>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insertion_position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<Vec<QueueWorkspace>>,
}

impl CardQueue {
    pub fn empty() -> Self {
        Self {
            version: 1,
            revision: 0,
            cards: vec![],
            order: vec![],
            turn_tags_enabled: Some(false),
            sort_mode: Some("score".into()),
            turn_tag_definitions: None,
            insertion_position: Some("bottom".into()),
            workspaces: Some(vec![]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHost {
    pub id: String,
    pub name: String,
    pub hostname: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
}

fn default_powershell() -> bool {
    cfg!(windows)
}

fn default_developer_probes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_powershell")]
    pub powershell_enabled: bool,
    #[serde(default = "default_developer_probes", alias = "developerProbes")]
    pub developer_probes: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            powershell_enabled: default_powershell(),
            developer_probes: default_developer_probes(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_float_weights_from_queue_json() {
        let raw = r#"{
            "version": 1,
            "revision": 1,
            "cards": [{
                "id": "c1",
                "cwd": "/",
                "phase": "draft",
                "createdAt": 1,
                "priorityWeight": 0.0
            }],
            "order": [],
            "workspaces": [{
                "id": "w1",
                "name": "Home",
                "kind": "local",
                "cwd": "/",
                "runtimeCwd": "/",
                "defaultConversationWeight": 0.0
            }]
        }"#;
        let queue: CardQueue = serde_json::from_str(raw).unwrap();
        assert_eq!(queue.cards[0].priority_weight, Some(0));
        assert_eq!(queue.workspaces.unwrap()[0].default_conversation_weight, Some(0));
    }

    #[test]
    fn powershell_defaults_on_windows_only() {
        let missing: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.powershell_enabled, cfg!(windows));
        let explicit: AppSettings = serde_json::from_str(r#"{"powershell_enabled":false}"#).unwrap();
        assert!(!explicit.powershell_enabled);
        assert_eq!(AppSettings::default().powershell_enabled, cfg!(windows));
        assert!(missing.developer_probes);
        let off: AppSettings = serde_json::from_str(r#"{"developer_probes":false}"#).unwrap();
        assert!(!off.developer_probes);
        let camel: AppSettings = serde_json::from_str(r#"{"developerProbes":false}"#).unwrap();
        assert!(!camel.developer_probes);
    }
}
