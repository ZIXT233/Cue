//! Sessions Cue never launched.
//!
//! Cursor's user-level `hooks.json` is global, so the same ingress also fires for
//! IDE chats and terminals this app has no terminal id for. Those sessions are not
//! queue cards — no workspace, no pty, nothing to persist — but they still reach
//! attention, so instead of dropping their events they are collected here and
//! surfaced as a transient notice that disappears the moment the session works again.

use super::signals::{observe_hook, HookSignal, ProbeState};
use crate::live::LiveBus;
use crate::models::ExternalNotice;
use crate::paths::external_signal_dir;
use parking_lot::Mutex;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Working is the real retraction signal. This only retires a notice whose session
/// stopped reporting altogether — an IDE closed mid-prompt never sends one.
const NOTICE_TTL_MS: i64 = 60 * 60 * 1000;
/// Files this old predate the running app; replaying them would resurrect notices
/// the user already dealt with.
const SIGNAL_MAX_AGE_MS: i64 = 10 * 60 * 1000;
const MAX_NOTICES: usize = 8;
const MAX_FILES_PER_TICK: usize = 200;
const POLL_MS: u64 = 500;

struct Tracked {
    notice: ExternalNotice,
    /// Last event from this session whatever its state, so an uninterrupted
    /// attention wait is not mistaken for a dead session.
    seen_at: i64,
    /// The user closed this notice by hand. Suppressed until the session reports a
    /// newer attention event, which is a genuinely new ask rather than the same one.
    dismissed_at: Option<i64>,
}

#[derive(Clone)]
pub struct ExternalRuntime {
    notices: Arc<Mutex<HashMap<String, Tracked>>>,
}

impl ExternalRuntime {
    pub fn new(live: LiveBus) -> Self {
        let notices: Arc<Mutex<HashMap<String, Tracked>>> = Arc::new(Mutex::new(HashMap::new()));
        let probes: Arc<Mutex<HashMap<String, ProbeState>>> = Arc::new(Mutex::new(HashMap::new()));
        let watch_notices = notices.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(POLL_MS));
            if drain(&probes, &watch_notices) {
                live.notify("external");
            }
        });
        Self { notices }
    }

    /// Notices currently on screen, oldest first. Injected into the queue snapshot
    /// on read; `queue.json` never learns about them.
    pub fn notices(&self) -> Vec<ExternalNotice> {
        let mut list: Vec<ExternalNotice> = self.notices.lock().values()
            .filter(|tracked| tracked.dismissed_at.is_none())
            .map(|tracked| tracked.notice.clone())
            .collect();
        list.sort_by_key(|notice| notice.at);
        if list.len() > MAX_NOTICES {
            list.drain(..list.len() - MAX_NOTICES);
        }
        list
    }

    /// Take a notice off screen at the user's request. The tracking entry survives so
    /// the same attention event cannot immediately re-raise it; only a newer event
    /// from that session brings it back.
    pub fn dismiss(&self, id: &str) -> bool {
        let mut map = self.notices.lock();
        match map.get_mut(id) {
            Some(tracked) if tracked.dismissed_at.is_none() => {
                tracked.dismissed_at = Some(now_ms());
                true
            }
            _ => false,
        }
    }
}

fn drain(probes: &Mutex<HashMap<String, ProbeState>>, notices: &Mutex<HashMap<String, Tracked>>) -> bool {
    // Expire first: a missing sink directory must not freeze notices on screen.
    let mut changed = expire(probes, notices);
    let Ok(entries) = std::fs::read_dir(external_signal_dir()) else { return changed };
    let mut files: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| signal_name().is_match(name))
        })
        .collect();
    files.sort();
    for path in files.into_iter().take(MAX_FILES_PER_TICK) {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(signal) = serde_json::from_str::<HookSignal>(&raw) {
                changed |= apply(probes, notices, signal);
            }
        }
        // A malformed file is consumed too: retrying it forever would block the sink.
        let _ = std::fs::remove_file(&path);
    }
    changed
}

fn apply(probes: &Mutex<HashMap<String, ProbeState>>, notices: &Mutex<HashMap<String, Tracked>>, signal: HookSignal) -> bool {
    let now = now_ms();
    if signal.agent_id.is_some() || now - signal.at > SIGNAL_MAX_AGE_MS {
        return false;
    }
    let Some(key) = notice_key(&signal) else { return false };
    let state = {
        let mut map = probes.lock();
        let current = map.get(&key).cloned().unwrap_or_default();
        let next = observe_hook(current, signal.clone());
        let state = next.state.clone();
        map.insert(key.clone(), next);
        state
    };
    let mut map = notices.lock();
    // A closed chat leaves nothing to attend to, even though `sessionEnd` folds into
    // the same Stop state a finished turn does.
    if signal.event == "sessionEnd" {
        return map.remove(&key).is_some();
    }
    if state == "working" {
        return map.remove(&key).is_some();
    }
    if state != "attention" {
        if let Some(tracked) = map.get_mut(&key) {
            tracked.seen_at = now;
        }
        return false;
    }
    // A hand-dismissed notice stays down until this session asks something newer;
    // re-raising the exact event the user just closed would be a fight, not a feature.
    if let Some(tracked) = map.get_mut(&key) {
        if tracked.dismissed_at.is_some() {
            tracked.seen_at = now;
            if signal.at <= tracked.notice.at {
                return false;
            }
        }
    }
    let notice = ExternalNotice {
        id: key.clone(),
        kind: signal.kind.clone().unwrap_or_else(|| "cursor".into()),
        session_id: signal.session_id.clone(),
        project: signal.workspace_root.as_deref().and_then(project_name),
        state,
        preview: notice_preview(&signal),
        notification: signal.notification.clone(),
        tool: signal.tool.clone(),
        at: signal.at,
    };
    // A newer event clears the dismissal, so `changed` must account for the notice
    // becoming visible again even when its own fields are identical.
    let changed = map.get(&key).is_none_or(|tracked| tracked.notice != notice || tracked.dismissed_at.is_some());
    map.insert(key, Tracked { notice, seen_at: now, dismissed_at: None });
    changed
}

fn expire(probes: &Mutex<HashMap<String, ProbeState>>, notices: &Mutex<HashMap<String, Tracked>>) -> bool {
    let now = now_ms();
    let (changed, live): (bool, HashSet<String>) = {
        let mut map = notices.lock();
        let before = map.len();
        map.retain(|_, tracked| now - tracked.seen_at < NOTICE_TTL_MS);
        (map.len() != before, map.keys().cloned().collect())
    };
    // Sessions that no longer show a notice need no bookkeeping; the next attention
    // event rebuilds it from scratch.
    probes.lock().retain(|key, _| live.contains(key));
    changed
}

/// Stable across reloads, and distinct for two chats open in the same workspace.
fn notice_key(signal: &HookSignal) -> Option<String> {
    if let Some(id) = signal.session_id.as_deref() {
        if session_id_pattern().is_match(id) {
            return Some(id.to_string());
        }
    }
    let root = signal.workspace_root.as_deref()?.trim_end_matches(['/', '\\']);
    (!root.is_empty()).then(|| format!("path:{root}"))
}

fn notice_preview(signal: &HookSignal) -> Option<String> {
    let text = signal.reply_preview.as_ref().or(signal.prompt.as_ref())?;
    let clipped: String = text.chars().filter(|c| !c.is_control()).take(160).collect();
    let clipped = clipped.trim();
    (!clipped.is_empty()).then(|| clipped.to_string())
}

fn project_name(root: &str) -> Option<String> {
    let trimmed = root.trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next()?;
    (!name.is_empty()).then(|| name.to_string())
}

fn signal_name() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^\d+-[a-f0-9-]+\.json$").unwrap())
}

fn session_id_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$").unwrap())
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(event: &str, at: i64) -> HookSignal {
        HookSignal {
            kind: Some("cursor".into()),
            at,
            event: event.into(),
            session_id: Some("c6553b99-eef0-4d2a-af62-8deaa625f841".into()),
            workspace_root: Some("/home/u/Projects/cue".into()),
            external: Some(true),
            ..HookSignal::default()
        }
    }

    fn store() -> (Mutex<HashMap<String, ProbeState>>, Mutex<HashMap<String, Tracked>>) {
        (Mutex::new(HashMap::new()), Mutex::new(HashMap::new()))
    }

    fn now() -> i64 {
        now_ms()
    }

    fn only(notices: &Mutex<HashMap<String, Tracked>>) -> ExternalNotice {
        notices.lock().values().next().map(|tracked| tracked.notice.clone()).expect("one notice")
    }

    /// Mirrors `ExternalRuntime::notices`, which needs a running watcher to build.
    fn visible(notices: &Mutex<HashMap<String, Tracked>>) -> Vec<ExternalNotice> {
        let mut list: Vec<ExternalNotice> = notices.lock().values()
            .filter(|tracked| tracked.dismissed_at.is_none())
            .map(|tracked| tracked.notice.clone())
            .collect();
        list.sort_by_key(|notice| notice.at);
        list
    }

    fn dismiss(notices: &Mutex<HashMap<String, Tracked>>, id: &str) -> bool {
        match notices.lock().get_mut(id) {
            Some(tracked) if tracked.dismissed_at.is_none() => {
                tracked.dismissed_at = Some(now_ms());
                true
            }
            _ => false,
        }
    }

    #[test]
    fn attention_raises_and_working_retracts() {
        let (probes, notices) = store();
        let at = now();
        assert!(apply(&probes, &notices, signal("preToolUse", at)));
        let raised = only(&notices);
        assert_eq!(raised.project.as_deref(), Some("cue"));
        assert_eq!(raised.state, "attention");
        assert_eq!(raised.id, "c6553b99-eef0-4d2a-af62-8deaa625f841");
        // The same attention event must not churn the notice (and its SSE refresh).
        assert!(!apply(&probes, &notices, signal("preToolUse", at)));
        assert!(apply(&probes, &notices, signal("beforeSubmitPrompt", at + 1)));
        assert!(notices.lock().is_empty());
    }

    #[test]
    fn a_finished_turn_raises_a_notice() {
        let (probes, notices) = store();
        let mut stop = signal("stop", now());
        stop.reply_preview = Some("改好了，顺便补了测试。".into());
        assert!(apply(&probes, &notices, stop));
        let raised = only(&notices);
        assert_eq!(raised.preview.as_deref(), Some("改好了，顺便补了测试。"));
        assert_eq!(raised.notification, None);
    }

    #[test]
    fn a_closed_chat_retracts_instead_of_raising() {
        let (probes, notices) = store();
        assert!(apply(&probes, &notices, signal("stop", now())));
        assert!(apply(&probes, &notices, signal("sessionEnd", now() + 1)));
        assert!(notices.lock().is_empty());
    }

    #[test]
    fn subagent_events_are_ignored() {
        let (probes, notices) = store();
        let mut child = signal("preToolUse", now());
        child.agent_id = Some("sub-1".into());
        assert!(!apply(&probes, &notices, child));
        assert!(notices.lock().is_empty());
    }

    #[test]
    fn stale_signals_are_skipped() {
        let (probes, notices) = store();
        assert!(!apply(&probes, &notices, signal("stop", now() - SIGNAL_MAX_AGE_MS - 1)));
        assert!(notices.lock().is_empty());
    }

    #[test]
    fn a_session_without_an_id_falls_back_to_its_workspace() {
        let (probes, notices) = store();
        let mut anonymous = signal("stop", now());
        anonymous.session_id = None;
        assert!(apply(&probes, &notices, anonymous));
        assert_eq!(notices.lock().keys().next().map(String::as_str), Some("path:/home/u/Projects/cue"));
    }

    #[test]
    fn project_is_the_last_path_segment() {
        assert_eq!(project_name("/home/u/Projects/cue").as_deref(), Some("cue"));
        assert_eq!(project_name(r"C:\Users\u\Projects\cue\\").as_deref(), Some("cue"));
        assert_eq!(project_name("/"), None);
    }

    #[test]
    fn expired_notices_release_their_probe() {
        let (probes, notices) = store();
        apply(&probes, &notices, signal("stop", now()));
        assert_eq!(probes.lock().len(), 1);
        notices.lock().values_mut().for_each(|tracked| tracked.seen_at = now() - NOTICE_TTL_MS - 1);
        assert!(expire(&probes, &notices));
        assert!(notices.lock().is_empty());
        assert!(probes.lock().is_empty());
    }

    #[test]
    fn a_dismissed_notice_stays_down_for_the_same_ask() {
        let (probes, notices) = store();
        let at = now();
        apply(&probes, &notices, signal("preToolUse", at));
        assert_eq!(visible(&notices).len(), 1);
        assert!(dismiss(&notices, "c6553b99-eef0-4d2a-af62-8deaa625f841"));
        assert!(visible(&notices).is_empty());
        assert!(!dismiss(&notices, "c6553b99-eef0-4d2a-af62-8deaa625f841"));
        // Replaying the very event the user closed must not fight them.
        assert!(!apply(&probes, &notices, signal("preToolUse", at)));
        assert!(visible(&notices).is_empty());
    }

    #[test]
    fn a_newer_ask_clears_the_dismissal() {
        let (probes, notices) = store();
        let at = now();
        apply(&probes, &notices, signal("preToolUse", at));
        dismiss(&notices, "c6553b99-eef0-4d2a-af62-8deaa625f841");
        assert!(visible(&notices).is_empty());
        // A fresh attention event is a new ask, so the notice earns its way back.
        assert!(apply(&probes, &notices, signal("stop", at + 1)));
        assert_eq!(visible(&notices).len(), 1);
    }

    #[test]
    fn working_after_a_dismissal_clears_the_tracking() {
        let (probes, notices) = store();
        let at = now();
        apply(&probes, &notices, signal("preToolUse", at));
        dismiss(&notices, "c6553b99-eef0-4d2a-af62-8deaa625f841");
        assert!(apply(&probes, &notices, signal("beforeSubmitPrompt", at + 1)));
        assert!(notices.lock().is_empty());
        // The session went back to work, so the next ask raises a plain notice again.
        assert!(apply(&probes, &notices, signal("preToolUse", at + 2)));
        assert_eq!(visible(&notices).len(), 1);
    }
}
