// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod commands;
pub mod config;
pub mod elevenlabs;
mod history;
pub mod local_stt;
pub mod local_tts;
mod mcp_register;
pub mod models;
mod runtime;
mod session;
pub mod voice_settings;
mod ws_server;

use std::sync::Arc;

use reqwest::Client;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

use session::SessionManager;

/// Show, unminimize, and focus the main window (used on ring and from the tray).
fn reveal_main(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    tauri::Builder::default()
        // Single-instance must be registered first so a second launch simply
        // surfaces the running app (which owns the WS port + runtime.json).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            reveal_main(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = Arc::new(SessionManager::new());
            app.manage(state.clone());
            // Shared HTTP client for ElevenLabs (connection reuse).
            app.manage(Client::new());

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

            build_tray(app.handle())?;

            // Closing the window hides it to the tray so the app stays always-on
            // (the agent can still ring it). Quit explicitly via the tray menu.
            if let Some(win) = app.get_webview_window("main") {
                let win_for_event = win.clone();
                win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win_for_event.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::respond_call,
            commands::respond_listen,
            commands::respond_ack,
            commands::notify_hangup,
            commands::voice_config,
            commands::list_voices,
            commands::save_voice_config,
            commands::tts,
            commands::stt,
            commands::voice_settings,
            commands::set_stt_provider,
            commands::set_tts_provider,
            commands::set_local_voice,
            commands::list_local_voices,
            commands::model_status,
            commands::download_model,
            commands::delete_model,
            commands::mcp_clients,
            commands::mcp_status,
            commands::register_mcp,
            commands::unregister_mcp,
            commands::set_dnd,
            commands::get_dnd,
            commands::save_call,
            commands::list_calls,
            commands::clear_calls
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                runtime::remove();
            }
        });
}

/// Build the system-tray / menu-bar icon with Show and Quit actions.
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .tooltip("Agent Voice App")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => reveal_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
