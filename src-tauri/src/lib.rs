// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod commands;
mod runtime;
mod session;
mod ws_server;

use std::sync::Arc;

use tauri::Manager;

use session::SessionManager;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = Arc::new(SessionManager::new());
            app.manage(state.clone());

            let handle = app.handle().clone();
            match ws_server::start(handle, state.clone()) {
                Ok(port) => match runtime::write(port, state.token()) {
                    Ok(p) => eprintln!(
                        "voice-call: WS server on 127.0.0.1:{port}; runtime file at {}",
                        p.display()
                    ),
                    Err(e) => eprintln!(
                        "voice-call: WS server on 127.0.0.1:{port}, but runtime write failed: {e}"
                    ),
                },
                Err(e) => eprintln!("voice-call: failed to start WS server: {e}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::respond_call,
            commands::respond_listen,
            commands::respond_ack,
            commands::notify_hangup
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                runtime::remove();
            }
        });
}
