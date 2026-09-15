pub mod api;
pub mod debuglog;
mod cwd;
mod dev_tools;
mod error;
mod harness;
mod hosts;
mod live;
mod models;
mod notify;
mod paste;
mod paths;
mod queue;
mod remote;
mod settings;
mod ssh;
mod terminal;
mod transcript;
mod winproc;

use api::{build_state, start_server};
use std::sync::Mutex;
use tauri::Manager;

struct ApiPort(Mutex<u16>);

#[tauri::command]
fn api_base(port: tauri::State<ApiPort>) -> String {
    format!("http://127.0.0.1:{}", *port.0.lock().unwrap())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin registered: a second launch hands its argv to the
        // running instance and exits, instead of spawning a rival process that would
        // race on ~/.cue/queue.json, settings.json and the terminal registry.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            crate::debuglog::log(&format!(
                "cue starting; ssh-chain log: {} (CUE_DEBUG=0 silences)",
                crate::debuglog::log_path().display()
            ));
            let resource_dir = app.path().resource_dir().ok();
            let state = build_state(resource_dir);
            app.manage(state.terminals.clone());
            let port = tauri::async_runtime::block_on(start_server(state)).map_err(|e| e.to_string())?;
            app.manage(ApiPort(Mutex::new(port)));
            notify::init(&app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api_base, notify::send_completion_notification, notify::open_notification_settings])
        .build(tauri::generate_context!())
        .expect("error while building Cue")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(hub) = app.try_state::<crate::terminal::TerminalHub>() {
                    hub.shutdown();
                }
            }
        });
}
