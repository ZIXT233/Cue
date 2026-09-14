use crate::cwd::{browse, pick_local_folder};
use crate::error::{AppError, AppResult};
use crate::harness::{session_exists, HarnessRuntime};
use crate::hosts::HostStore;
use crate::live::LiveBus;
use crate::models::{CardQueue, RemoteHost};
use crate::paths::{resolve_bin_dir, ssh_runtime_dir};
use crate::queue::{
    archive_card, defer_card, move_card, numeric_weight, now_ms, select_workspace_for_draft, sync_queue, QueueStore, TAB_LEASE_MS,
};
use crate::paste::{save_terminal_files, save_terminal_images, terminal_image_paste, validate_terminal_files, validate_terminal_images};
use crate::settings::SettingsStore;
use crate::ssh::{connect_host, shell_quote, ssh_exec, test_target};
use crate::terminal::{TerminalEvent, TerminalHub};
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub queue: Arc<QueueStore>,
    pub terminals: TerminalHub,
    pub harness: HarnessRuntime,
    pub hosts: Arc<HostStore>,
    pub settings: Arc<SettingsStore>,
    pub live: LiveBus,
    pub bin_dir: PathBuf,
    pub default_cwd: PathBuf,
    pub launches: Arc<Mutex<HashSet<String>>>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/card-queue", get(get_queue).post(post_queue))
        .route("/api/card-queue/bootstrap", get(bootstrap_queue))
        .route("/api/card-queue/events", get(queue_events))
        .route("/api/workspace-machines", get(list_machines).post(machine_action))
        .route("/api/cwd/browse", get(browse_cwd))
        .route("/api/tools/settings", get(get_tools).put(put_tools))
        .route("/api/harness/{id}/debug", get(harness_debug))
        .route("/api/terminal", post(create_terminal))
        .route("/api/terminal/{id}", get(get_terminal).post(post_terminal).delete(delete_terminal))
        .route("/api/terminal/{id}/events", get(terminal_events))
        .route("/api/sessions", get(empty_sessions))
        .layer(DefaultBodyLimit::max(110 * 1024 * 1024))
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .with_state(state)
}

fn overlay_harness(queue: &mut CardQueue, state: &AppState) {
    for card in &mut queue.cards {
        let Some(session) = card.harness.take() else { continue };
        let refresh = card.archived_at.is_none()
            || (session.provider_session_id.is_none() && session.unpersisted_session.is_none());
        card.harness = Some(if refresh {
            state.harness.snapshot(&session, &state.terminals)
        } else {
            session
        });
    }
}

fn refresh_queue(queue: &mut CardQueue, state: &AppState) {
    overlay_harness(queue, state);
    sync_queue(queue, &state.terminals);
}

async fn bootstrap_queue(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    Ok(Json(with_cwd(state.queue.read_snapshot()?, &state)))
}

async fn get_queue(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let queue = state.queue.with_queue(true, |queue| {
        refresh_queue(queue, &state);
        Ok(queue.clone())
    }).await?;
    Ok(Json(with_cwd(queue, &state)))
}

async fn post_queue(State(state): State<AppState>, Json(mut body): Json<Value>) -> AppResult<impl IntoResponse> {
    let action = body.get("action").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if matches!(action.as_str(), "harness_start" | "harness_reopen" | "harness_restart" | "harness_resume") {
        return launch_harness(&state, &body, &action).await.map(Json);
    }
    if action == "workspace_create" && body.get("kind").and_then(|v| v.as_str()) == Some("ssh") {
        let host = body.get("sshHost").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let cwd_in = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._@:-]*$").unwrap().is_match(&host) && cwd_in.starts_with('/') {
            let resolved = String::from_utf8_lossy(
                &ssh_exec(&host, &format!("cd {} && pwd -P", shell_quote(&cwd_in))).await?,
            ).trim().to_string();
            if resolved.starts_with('/') {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("cwd".into(), json!(resolved));
                }
            } else {
                return Err(AppError::msg("无法解析远程工作目录"));
            }
        }
    }
    let queue = state.queue.with_queue(false, |queue| {
        refresh_queue(queue, &state);
        apply_action(queue, &body, &action, &state)?;
        refresh_queue(queue, &state);
        Ok(queue.clone())
    }).await?;
    Ok(Json(queue))
}

struct LaunchGuard {
    launches: Arc<Mutex<HashSet<String>>>,
    id: String,
}

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        self.launches.lock().remove(&self.id);
    }
}

fn try_acquire_launch(state: &AppState, id: &str) -> AppResult<LaunchGuard> {
    let mut launches = state.launches.lock();
    if !launches.insert(id.to_string()) {
        return Err(AppError::msg("此卡片正在启动，请等待启动结果"));
    }
    Ok(LaunchGuard { launches: state.launches.clone(), id: id.to_string() })
}

fn launch_identity_changed(
    card: &crate::models::QueueCard,
    workspace: &crate::models::QueueWorkspace,
    captured_card: &crate::models::QueueCard,
    captured_workspace: &crate::models::QueueWorkspace,
    previous_terminal: Option<&str>,
) -> bool {
    card.workspace_id != captured_card.workspace_id
        || card.cwd != captured_card.cwd
        || card.session.as_ref().map(|s| s.id.as_str()) != captured_card.session.as_ref().map(|s| s.id.as_str())
        || card.harness.as_ref().map(|h| h.terminal_id.as_str()) != previous_terminal
        || card.archived_at != captured_card.archived_at
        || workspace.kind != captured_workspace.kind
        || workspace.cwd != captured_workspace.cwd
        || workspace.runtime_cwd != captured_workspace.runtime_cwd
        || workspace.ssh_host != captured_workspace.ssh_host
}

async fn launch_harness(state: &AppState, body: &Value, action: &str) -> AppResult<CardQueue> {
    let id = body.get("id").and_then(|v| v.as_str()).ok_or_else(|| AppError::msg("卡片 ID 无效"))?;
    let _guard = try_acquire_launch(state, id)?;
    let captured = state.queue.with_queue(true, |queue| {
        refresh_queue(queue, state);
        let card = queue.cards.iter().find(|c| c.id == id).cloned().ok_or_else(|| AppError::msg("卡片已不存在"))?;
        if action == "harness_start" {
            if card.session.is_some() || card.harness.is_some() {
                return Err(AppError::msg("请选择空白卡片"));
            }
        } else {
            let harness = card.harness.as_ref().ok_or_else(|| AppError::msg("请先退出当前 CLI"))?;
            let dead = state.terminals.snapshot(&harness.terminal_id).is_none_or(|t| t.exited);
            if !["error", "exited"].contains(&harness.state.as_str()) && !dead {
                return Err(AppError::msg("请先退出当前 CLI"));
            }
            if action == "harness_reopen" && harness.provider_session_id.is_some() {
                return Err(AppError::msg("已识别原会话，请使用继续会话"));
            }
        }
        let workspace = queue.workspaces.as_ref().and_then(|ws| ws.iter().find(|w| Some(&w.id) == card.workspace_id.as_ref())).cloned().ok_or_else(|| AppError::msg("工作区不存在"))?;
        Ok((card, workspace))
    }).await?;
    let kind = if action == "harness_start" {
        body.get("kind").and_then(|v| v.as_str()).unwrap_or_default()
    } else {
        captured.0.harness.as_ref().map(|h| h.kind.as_str()).unwrap_or_default()
    };
    let resume = if action == "harness_start" || action == "harness_reopen" {
        None
    } else {
        captured.0.harness.clone().filter(|session| {
            session.remote == Some(true)
                || session.provider_session_id.as_deref().is_some_and(|id| session_exists(&session.kind, id) != Some(false))
        })
    };
    let launched = state.harness.launch(kind, &captured.1, resume.clone(), &state.terminals, &state.settings, &state.bin_dir).await?;
    let previous_id = captured.0.harness.as_ref().map(|h| h.terminal_id.clone());
    let result = state.queue.with_queue(false, |queue| {
        let workspace = queue.workspaces.as_ref().and_then(|ws| ws.iter().find(|w| w.id == captured.1.id)).cloned();
        let card = queue.cards.iter_mut().find(|c| c.id == id);
        let changed = match (card.as_ref(), workspace.as_ref()) {
            (Some(card), Some(workspace)) => launch_identity_changed(card, workspace, &captured.0, &captured.1, previous_id.as_deref()),
            _ => true,
        };
        if changed {
            return Err(AppError::msg("启动期间卡片或工作区已变更，请重试"));
        }
        let card = card.unwrap();
        card.harness = Some(launched.clone());
        card.phase = crate::models::CardPhase::Attention;
        card.archived_at = None;
        if !queue.order.contains(&card.id) {
            queue.order.push(card.id.clone());
        }
        refresh_queue(queue, state);
        Ok(queue.clone())
    }).await;
    if result.is_ok() {
        if let Some(id) = previous_id { state.terminals.kill(&id); }
    } else {
        state.terminals.kill(&launched.terminal_id);
    }
    result
}

fn valid_side_terminal_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| matches!(c, 'a'..='f' | '0'..='9'))
}

fn kill_side_terminals(state: &AppState, card: &crate::models::QueueCard) {
    if let Some(tabs) = &card.side_terminals {
        for tab in tabs {
            state.terminals.kill(&tab.id);
        }
    }
}

fn apply_action(queue: &mut CardQueue, body: &Value, action: &str, state: &AppState) -> AppResult<()> {
    let id = body.get("id").and_then(|v| v.as_str());
    match action {
        "shell_background" => {
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("CLI 卡片不存在"))?;
            let harness = card.harness.as_mut().ok_or_else(|| AppError::msg("CLI 卡片不存在"))?;
            if harness.kind != "shell" || harness.shell_command_notifications == Some(false) || harness.shell_command_running != Some(true) {
                return Err(AppError::msg("命令已结束或尚未运行 300ms"));
            }
            if now_ms() - harness.shell_command_started_at.unwrap_or(now_ms()) < 300 {
                return Err(AppError::msg("命令已结束或尚未运行 300ms"));
            }
            harness.shell_notify = Some(true);
        }
        "harness_close" => {
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("CLI 卡片不存在"))?;
            kill_side_terminals(state, card);
            card.side_terminals = None;
            card.side_terminal_open = None;
            if let Some(harness) = card.harness.as_mut() {
                state.terminals.stop(&harness.terminal_id);
                harness.state = "exited".into();
            }
            card.phase = crate::models::CardPhase::Attention;
            card.detached = None;
            archive_card(queue, id.unwrap())?;
        }
        "sort_mode" => {
            let mode = body.get("mode").and_then(|v| v.as_str()).unwrap_or_default();
            if !["fifo", "score"].contains(&mode) { return Err(AppError::msg("无效排序模式")); }
            queue.sort_mode = Some(mode.into());
            queue.insertion_position = Some("bottom".into());
        }
        "turn_tags_enabled" => queue.turn_tags_enabled = body.get("enabled").and_then(|v| v.as_bool()),
        "turn_tags" => {
            queue.turn_tag_definitions = serde_json::from_value(body.get("tags").cloned().unwrap_or(json!([]))).ok();
        }
        "workspace_weight" => {
            let workspace_id = body.get("workspaceId").and_then(|v| v.as_str()).unwrap_or_default();
            let workspace = queue.workspaces.as_mut().and_then(|ws| ws.iter_mut().find(|w| w.id == workspace_id)).ok_or_else(|| AppError::msg("工作区不存在"))?;
            workspace.default_conversation_weight = Some(numeric_weight(body.get("weight").unwrap_or(&json!(0))));
        }
        "workspace_update" => {
            let workspace_id = body.get("workspaceId").and_then(|v| v.as_str()).unwrap_or_default();
            let workspace = queue.workspaces.as_mut().and_then(|ws| ws.iter_mut().find(|w| w.id == workspace_id)).ok_or_else(|| AppError::msg("工作区不存在"))?;
            let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
            if name.is_empty() { return Err(AppError::msg("请输入工作区名称")); }
            workspace.name = name.into();
            workspace.default_conversation_weight = Some(numeric_weight(body.get("defaultConversationWeight").unwrap_or(&json!(0))));
        }
        "priority_weight" => {
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("卡片不存在"))?;
            card.priority_weight = Some(numeric_weight(body.get("weight").unwrap_or(&json!(0))));
        }
        "reset_wait" => {
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("卡片未在等待处理"))?;
            if !matches!(card.phase, crate::models::CardPhase::Attention) {
                return Err(AppError::msg("卡片未在等待处理"));
            }
            card.waiting_since = Some(now_ms());
        }
        "insertion_position" => {
            let position = body.get("position").and_then(|v| v.as_str()).unwrap_or_default();
            if !["top", "bottom"].contains(&position) { return Err(AppError::msg("请选择顶部插入或底部插入")); }
            queue.insertion_position = Some(position.into());
        }
        "workspace_create" => {
            let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let cwd_in = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let kind = body.get("kind").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            if name.is_empty() { return Err(AppError::msg("请输入工作区名称")); }
            if cwd_in.is_empty() { return Err(AppError::msg("请输入工作区目录")); }
            if !["local", "ssh"].contains(&kind.as_str()) { return Err(AppError::msg("请选择工作区位置")); }
            let (cwd, ssh_host, runtime_cwd, id) = if kind == "local" {
                let cwd = crate::paths::expand_user(&cwd_in);
                if !cwd.is_dir() { return Err(AppError::msg("工作目录不存在")); }
                (cwd.to_string_lossy().into_owned(), None, cwd.to_string_lossy().into_owned(), Uuid::new_v4().to_string())
            } else {
                let host = body.get("sshHost").and_then(|v| v.as_str()).unwrap_or_default();
                if !regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._@:-]*$").unwrap().is_match(host) {
                    return Err(AppError::msg("请输入 SSH 主机别名或 user@host"));
                }
                if !cwd_in.starts_with('/') { return Err(AppError::msg("SSH 工作目录请使用绝对路径")); }
                let resolved = cwd_in.clone();
                let id = Uuid::new_v4().to_string();
                let runtime = ssh_runtime_dir(&id);
                std::fs::create_dir_all(&runtime)?;
                std::fs::write(runtime.join("remote-workspace.json"), serde_json::json!({ "sshHost": host, "cwd": resolved }).to_string())?;
                (resolved, Some(host.to_string()), runtime.to_string_lossy().into_owned(), id)
            };
            let create_card = body.get("createCard").and_then(|v| v.as_bool()) == Some(true);
            let workspace = {
                let workspaces = queue.workspaces.get_or_insert_with(Vec::new);
                if let Some(existing) = workspaces.iter_mut().find(|w| w.kind == kind && w.cwd == cwd && w.ssh_host == ssh_host) {
                    existing.name = name;
                    existing.default_conversation_weight = Some(numeric_weight(body.get("defaultConversationWeight").unwrap_or(&json!(0))));
                    existing.clone()
                } else {
                    let workspace = crate::models::QueueWorkspace {
                        id,
                        name,
                        kind,
                        cwd,
                        ssh_host,
                        runtime_cwd,
                        default_conversation_weight: Some(numeric_weight(body.get("defaultConversationWeight").unwrap_or(&json!(0)))),
                    };
                    workspaces.push(workspace.clone());
                    workspace
                }
            };
            if create_card {
                select_workspace_for_draft(queue, &workspace);
            }
        }
        "workspace_remove" => {
            let workspace_id = body.get("workspaceId").and_then(|v| v.as_str()).unwrap_or_default();
            let removed: Vec<String> = queue.cards.iter().filter(|c| c.workspace_id.as_deref() == Some(workspace_id)).map(|c| c.id.clone()).collect();
            for card in &queue.cards {
                if removed.contains(&card.id) {
                    if let Some(harness) = &card.harness { state.terminals.kill(&harness.terminal_id); }
                    kill_side_terminals(state, card);
                }
            }
            queue.cards.retain(|c| !removed.contains(&c.id));
            queue.order.retain(|id| !removed.contains(id));
            if let Some(ws) = queue.workspaces.as_mut() { ws.retain(|w| w.id != workspace_id); }
        }
        "create" => {
            let workspace_id = body.get("workspaceId").and_then(|v| v.as_str()).unwrap_or_default();
            let workspace = queue.workspaces.as_ref().and_then(|ws| ws.iter().find(|w| w.id == workspace_id)).cloned().ok_or_else(|| AppError::msg("请选择一个工作区"))?;
            select_workspace_for_draft(queue, &workspace);
        }
        "defer" => defer_card(queue, id.unwrap_or_default()),
        "front" | "back" => move_card(queue, id.unwrap_or_default(), action),
        "archive" => {
            if let Some(card) = queue.cards.iter().find(|c| Some(c.id.as_str()) == id) {
                if let Some(harness) = &card.harness { state.terminals.stop(&harness.terminal_id); }
                kill_side_terminals(state, card);
            }
            archive_card(queue, id.unwrap_or_default())?;
            if let Some(card) = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id) {
                if let Some(harness) = card.harness.as_mut() { harness.state = "exited".into(); }
                card.side_terminals = None;
                card.side_terminal_open = None;
            }
        }
        "restore" => {
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("卡片已不存在"))?;
            card.archived_at = None;
            move_card(queue, id.unwrap_or_default(), "front");
        }
        "remove" => {
            if let Some(card) = queue.cards.iter().find(|c| Some(c.id.as_str()) == id) {
                if matches!(card.phase, crate::models::CardPhase::Working) { return Err(AppError::msg("请先处理等待中的交互，或停止正在运行的会话")); }
                if card.detached.is_some() { return Err(AppError::msg("请先收回独立窗口")); }
                if let Some(harness) = &card.harness { state.terminals.kill(&harness.terminal_id); }
                kill_side_terminals(state, card);
            }
            queue.cards.retain(|c| Some(c.id.as_str()) != id);
            queue.order.retain(|item| Some(item.as_str()) != id);
        }
        "claim" => {
            let owner = body.get("owner").and_then(|v| v.as_str()).unwrap_or_default();
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("无效的窗口"))?;
            if owner.is_empty() { return Err(AppError::msg("无效的窗口")); }
            if let Some(detached) = &card.detached {
                if detached.owner != owner { return Err(AppError::msg("该卡片已经在另一个窗口打开")); }
            }
            card.detached = Some(crate::models::DetachedLease { owner: owner.into(), expires_at: now_ms() + TAB_LEASE_MS });
        }
        "release" => {
            let owner = body.get("owner").and_then(|v| v.as_str()).unwrap_or_default();
            if let Some(card) = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id) {
                if card.detached.as_ref().is_some_and(|d| d.owner == owner) { card.detached = None; }
            }
        }
        "side_terminal_add" => {
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("卡片已不存在"))?;
            let terminal_id = body.get("terminalId").and_then(|v| v.as_str()).unwrap_or_default();
            if !valid_side_terminal_id(terminal_id) { return Err(AppError::msg("无效的终端")); }
            let cwd = body.get("cwd").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(card.cwd.as_str()).to_string();
            let tabs = card.side_terminals.get_or_insert_with(Vec::new);
            if !tabs.iter().any(|tab| tab.id == terminal_id) {
                tabs.push(crate::models::CardSideTerminal { id: terminal_id.into(), cwd });
            }
        }
        "side_terminal_remove" => {
            let terminal_id = body.get("terminalId").and_then(|v| v.as_str()).unwrap_or_default();
            if !valid_side_terminal_id(terminal_id) { return Err(AppError::msg("无效的终端")); }
            if let Some(card) = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id) {
                if let Some(tabs) = card.side_terminals.as_mut() {
                    tabs.retain(|tab| tab.id != terminal_id);
                    if tabs.is_empty() {
                        card.side_terminals = None;
                        card.side_terminal_open = None;
                    }
                }
            }
            state.terminals.kill(terminal_id);
        }
        "side_terminal_open" => {
            let card = queue.cards.iter_mut().find(|c| Some(c.id.as_str()) == id).ok_or_else(|| AppError::msg("卡片已不存在"))?;
            let open = body.get("open").and_then(|v| v.as_bool()).unwrap_or(false);
            card.side_terminal_open = open.then_some(true);
        }
        "adopt" | "attach" | "prompt_sources" => return Err(AppError::msg("Cue 不托管 Pi 原生会话")),
        _ => return Err(AppError::msg("未知队列操作")),
    }
    Ok(())
}

async fn queue_events(State(state): State<AppState>) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.live.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| {
        item.ok().map(|reason| Ok(Event::default().data(json!({ "type": "change", "reason": reason }).to_string())))
    });
    Sse::new(stream::once(async { Ok(Event::default().comment("")) }).chain(stream)).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}

async fn list_machines(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    Ok(Json(json!({ "hosts": state.hosts.list().await? })))
}

async fn machine_action(State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    match machine_action_inner(&state, body).await {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn machine_action_inner(state: &AppState, body: Value) -> AppResult<Value> {
    let action = body.get("action").and_then(|v| v.as_str()).unwrap_or_default();
    match action {
        "save" => {
            let host: RemoteHost = serde_json::from_value(body.get("host").cloned().unwrap_or(json!({})))?;
            Ok(json!({ "host": state.hosts.save(host).await? }))
        }
        "delete" => {
            let id = body.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            state.hosts.delete(id).await?;
            Ok(json!({ "ok": true }))
        }
        "set-visibility" => {
            let host = body.get("host").and_then(|v| v.as_str()).ok_or_else(|| AppError::machine("HOST_INVALID"))?;
            let visible = body.get("visible").and_then(|v| v.as_bool()).ok_or_else(|| AppError::machine("HOST_INVALID"))?;
            state.hosts.set_visibility(host, visible).await?;
            Ok(json!({ "ok": true }))
        }
        "test" => {
            let target: RemoteHost = serde_json::from_value(body.get("host").cloned().unwrap_or(json!({})))?;
            test_target(target, password(&body)?, trusted(&body)).await?;
            Ok(json!({ "ok": true }))
        }
        "local-folder" => {
            let locale = body.get("locale").and_then(|v| v.as_str()).map(|s| s.to_string());
            match pick_local_folder(locale).await? {
                Some(cwd) => Ok(json!({ "cwd": cwd })),
                None => Ok(json!({ "cancelled": true })),
            }
        }
        "test-host" | "connect" => {
            let host = body.get("host").and_then(|v| v.as_str()).ok_or_else(|| AppError::machine("HOST_INVALID"))?;
            connect_host(host, password(&body)?, trusted(&body)).await?;
            if action == "connect" {
                let cwd = String::from_utf8_lossy(&ssh_exec(host, r#"printf "%s" "$HOME""#).await?).into_owned();
                return Ok(json!({ "cwd": cwd }));
            }
            Ok(json!({ "ok": true }))
        }
        "directories" => {
            let host = body.get("host").and_then(|v| v.as_str()).ok_or_else(|| AppError::machine("HOST_INVALID"))?;
            let path = body.get("path").and_then(|v| v.as_str()).ok_or_else(|| AppError::machine("PATH_INVALID"))?;
            if !path.starts_with('/') || path.contains('\0') { return Err(AppError::machine("PATH_INVALID")); }
            let slash = path.rfind('/').unwrap_or(0);
            let parent = &path[..=slash];
            let prefix = &path[slash + 1..];
            let output = ssh_exec(host, &format!("cd {} && for p in ./* ./.[!.]* ./..?*; do [ -d \"$p\" ] && printf '%s\\0' \"$p\"; done; true", crate::ssh::shell_quote(parent))).await?;
            let mut entries: Vec<String> = String::from_utf8_lossy(&output).split('\0').filter(|s| !s.is_empty()).map(|s| s.trim_start_matches("./").to_string()).filter(|s| s.starts_with(prefix)).collect();
            entries.sort();
            let truncated = entries.len() > 300;
            let directories: Vec<Value> = entries.into_iter().take(300).map(|name| json!({ "name": name, "path": format!("{parent}{name}/") })).collect();
            Ok(json!({ "directories": directories, "truncated": truncated }))
        }
        _ => Err(AppError::machine("REQUEST_FAILED")),
    }
}

fn password(body: &Value) -> AppResult<Option<String>> {
    match body.get("password") {
        None => Ok(None),
        Some(Value::String(value)) if value.len() <= 8192 && !value.chars().any(|c| c == '\n' || c == '\r' || c == '\0') => Ok(Some(value.clone())),
        _ => Err(AppError::machine("PASSWORD_INVALID")),
    }
}

fn trusted(body: &Value) -> Option<String> {
    body.get("trustedPrompt").and_then(|v| v.as_str()).map(|s| s.to_string())
}

#[derive(Deserialize)]
struct BrowseQuery { path: Option<String> }

async fn browse_cwd(Query(query): Query<BrowseQuery>) -> AppResult<impl IntoResponse> {
    Ok(Json(browse(query.path)?))
}

async fn get_tools(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let settings = state.settings.read()?;
    Ok(Json(json!({
        "isWindows": cfg!(windows),
        "powerShellEnabled": settings.powershell_enabled,
        "developerProbes": crate::dev_tools::probes_enabled(),
    })))
}

async fn put_tools(State(state): State<AppState>, Json(body): Json<Value>) -> AppResult<impl IntoResponse> {
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let settings = state.settings.set_powershell(enabled).await?;
    Ok(Json(json!({
        "isWindows": cfg!(windows),
        "powerShellEnabled": settings.powershell_enabled,
        "developerProbes": crate::dev_tools::probes_enabled(),
    })))
}

async fn harness_debug(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if !crate::dev_tools::probes_enabled() {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))).into_response();
    }
    Json(state.harness.debug_snapshot(&id, &state.terminals)).into_response()
}

async fn get_terminal(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.terminals.cwd(&id) {
        Some(cwd) => (StatusCode::OK, Json(json!({ "id": id, "cwd": cwd, "readOnly": state.terminals.snapshot(&id).map(|s| s.exited).unwrap_or(true) }))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "Terminal expired or closed" }))).into_response(),
    }
}

async fn create_terminal(State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    match create_terminal_inner(&state, &body) {
        Ok(id) => (StatusCode::OK, Json(json!({ "id": id }))).into_response(),
        Err(error) => error.into_response(),
    }
}

fn create_terminal_inner(state: &AppState, body: &Value) -> AppResult<String> {
    if let Some(id) = body.get("id") {
        let Some(id) = id.as_str() else {
            return Err(AppError::msg("Invalid terminal id"));
        };
        if id.len() != 32 || !id.chars().all(|c| matches!(c, 'a'..='f' | '0'..='9')) {
            return Err(AppError::msg("Invalid terminal id"));
        }
    }
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("").trim();
    if cwd.is_empty() {
        return Err(AppError::msg("cwd required"));
    }
    let cwd = resolve_terminal_cwd(cwd);
    if !cwd.is_dir() {
        return Err(AppError::msg("cwd must be a directory"));
    }
    let cols = json_dimension(body.get("cols")).unwrap_or(80);
    let rows = json_dimension(body.get("rows")).unwrap_or(24);
    let id = body.get("id").and_then(|v| v.as_str()).map(str::to_string);
    state.terminals.create_shell(cwd.to_string_lossy().into_owned(), cols, rows, id)
}

fn resolve_terminal_cwd(cwd: &str) -> PathBuf {
    let path = PathBuf::from(cwd);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
    }
}

fn json_dimension(value: Option<&Value>) -> Option<u16> {
    let value = value?;
    let number = match value {
        Value::Number(n) if n.is_u64() => n.as_u64()? as f64,
        Value::Number(n) if n.is_i64() => n.as_i64()? as f64,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    if number.fract() != 0.0 || !(2.0..=1000.0).contains(&number) {
        return None;
    }
    Some(number as u16)
}

fn json_int_size(value: Option<&Value>) -> Option<u16> {
    json_dimension(value)
}

fn live_terminal(state: &AppState, id: &str) -> Option<crate::terminal::TerminalSnapshot> {
    state.terminals.snapshot(id).filter(|snapshot| !snapshot.exited)
}

fn terminal_gone() -> axum::response::Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "Terminal expired or closed" }))).into_response()
}

async fn post_terminal(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> impl IntoResponse {
    match post_terminal_inner(&state, &id, request).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

async fn post_terminal_inner(state: &AppState, id: &str, request: Request) -> AppResult<axum::response::Response> {
    let content_type = request.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if content_type.starts_with("multipart/form-data") {
        let Some(snapshot) = live_terminal(state, id) else {
            return Ok(terminal_gone());
        };
        let mut multipart = Multipart::from_request(request, &state).await.map_err(|_| AppError::msg("Invalid terminal command"))?;
        let mut files = Vec::new();
        let mut bracketed = false;
        while let Some(field) = multipart.next_field().await.map_err(|e| AppError::msg(e.to_string()))? {
            match field.name() {
                Some("bracketed") => bracketed = field.text().await.map_err(|e| AppError::msg(e.to_string()))? == "true",
                Some("files") => {
                    let name = field.file_name().unwrap_or("").to_string();
                    let bytes = field.bytes().await.map_err(|e| AppError::msg(e.to_string()))?;
                    files.push((name, bytes.to_vec()));
                }
                _ => {}
            }
        }
        if let Some(error) = validate_terminal_files(&files) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response());
        }
        let paths = save_terminal_files(&snapshot.cwd, &files).await?;
        return if state.terminals.write(id, &terminal_image_paste(&paths, bracketed)) {
            Ok((StatusCode::OK, Json(json!({ "success": true }))).into_response())
        } else {
            Ok((StatusCode::CONFLICT, Json(json!({ "error": "Terminal closed while uploading files" }))).into_response())
        };
    }
    let bytes = axum::body::to_bytes(request.into_body(), 110 * 1024 * 1024).await.map_err(|e| AppError::msg(e.to_string()))?;
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    match value.get("type").and_then(|v| v.as_str()) {
        Some("images") => {
            if let Some(error) = validate_terminal_images(value.get("images").unwrap_or(&json!(null))) {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response());
            }
            let Some(snapshot) = live_terminal(state, id) else {
                return Ok(terminal_gone());
            };
            let images = value.get("images").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let paths = save_terminal_images(&snapshot.cwd, &images).await?;
            if state.terminals.write(id, &terminal_image_paste(&paths, value.get("bracketed") == Some(&json!(true)))) {
                Ok((StatusCode::OK, Json(json!({ "success": true }))).into_response())
            } else {
                Ok((StatusCode::CONFLICT, Json(json!({ "error": "Terminal closed while uploading images" }))).into_response())
            }
        }
        Some("input") => {
            let data = value.get("data").and_then(|v| v.as_str()).unwrap_or("");
            if data.len() > 64 * 1024 {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid terminal command" }))).into_response());
            }
            if state.terminals.write(id, data) {
                Ok((StatusCode::OK, Json(json!({ "success": true }))).into_response())
            } else {
                Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Terminal not found" }))).into_response())
            }
        }
        Some("resize") => {
            let Some(cols) = json_int_size(value.get("cols")) else {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid terminal command" }))).into_response());
            };
            let Some(rows) = json_int_size(value.get("rows")) else {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid terminal command" }))).into_response());
            };
            if state.terminals.resize(id, cols, rows) {
                Ok((StatusCode::OK, Json(json!({ "success": true }))).into_response())
            } else {
                Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Terminal not found" }))).into_response())
            }
        }
        _ => Ok((StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid terminal command" }))).into_response()),
    }
}

async fn delete_terminal(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    state.terminals.kill(&id);
    Json(json!({ "success": true }))
}

#[derive(Deserialize)]
struct TerminalEventsQuery {
    after: Option<String>,
}

async fn terminal_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TerminalEventsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let after = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .or(query.after.as_deref())
        .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
        .and_then(|s| s.parse().ok());
    let Some((output, rx, exited, code)) = state.terminals.subscribe(&id, after) else {
        return (StatusCode::NOT_FOUND, "Terminal not found").into_response();
    };
    let mut initial = vec![sse_event(&output)];
    if exited {
        initial.push(sse_event(&TerminalEvent::Exit { exit_code: code.unwrap_or(0) }));
    }
    let stream = stream::iter(initial.into_iter().map(Ok::<_, Infallible>)).chain(
        UnboundedReceiverStream::new(rx).map(|event| Ok(sse_event(&event))),
    );
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30))).into_response()
}

fn sse_event(event: &TerminalEvent) -> Event {
    let id = if let TerminalEvent::Output { offset, .. } = event { Some(offset.to_string()) } else { None };
    let mut ev = Event::default().data(serde_json::to_string(event).unwrap_or_else(|_| "{}".into()));
    if let Some(id) = id { ev = ev.id(id); }
    ev
}

async fn empty_sessions() -> impl IntoResponse {
    Json(json!({ "sessions": [] }))
}

fn with_cwd(queue: CardQueue, state: &AppState) -> Value {
    let mut value = serde_json::to_value(queue).unwrap_or(json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert("defaultCwd".into(), json!(state.default_cwd));
    }
    value
}

pub async fn start_server(state: AppState) -> AppResult<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(port)
}

pub fn build_state(resource_dir: Option<PathBuf>) -> AppState {
    let live = LiveBus::new();
    let settings = Arc::new(SettingsStore::new());
    let _ = settings.load_for_boot();
    AppState {
        queue: Arc::new(QueueStore::new(live.clone())),
        terminals: TerminalHub::new(live.clone()),
        harness: HarnessRuntime::new(live.clone()),
        hosts: Arc::new(HostStore::new()),
        settings,
        live,
        bin_dir: resolve_bin_dir(resource_dir),
        default_cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        launches: Arc::new(Mutex::new(HashSet::new())),
    }
}
