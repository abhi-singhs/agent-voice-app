// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod commands;
pub mod config;
pub mod elevenlabs;
mod history;
pub mod local_stt;
pub mod local_tts;
mod mcp_register;
pub mod models;
pub mod paths;
mod runtime;
mod session;
pub mod voice_settings;
mod ws_server;

use std::path::Path;
use std::sync::Arc;

use reqwest::Client;
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use session::SessionManager;

/// Launch argument the OS passes when auto-starting the app at login. Its
/// presence means "come up quietly in the tray" rather than opening the window.
const AUTOSTART_HIDDEN_ARG: &str = "--hidden";

/// Show, unminimize, and focus the main window (used on ring and from the tray).
fn reveal_main(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// True when this process was launched with the autostart "hidden" flag, i.e. the
/// OS started it at login. Manual launches don't carry the flag.
fn launched_hidden() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_HIDDEN_ARG)
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
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_HIDDEN_ARG]),
        ))
        .setup(|app| {
            // Remove stale app data from the old ~/.copilot location (no
            // migration); the app now stores everything under its own folder.
            paths::cleanup_legacy_copilot_data();

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

            // Default autostart to ON the first time the app runs; a marker file
            // makes this a one-time action so a user who later turns it off from
            // the tray isn't re-enabled on the next launch. Do this before
            // build_tray so the "Start at Login" checkmark reflects the state.
            ensure_autostart_default(app.handle());

            build_tray(app.handle())?;

            // The window is created hidden (see tauri.conf.json). Show it now for a
            // normal launch, but stay hidden in the tray when the OS auto-started
            // us at login (the `--hidden` flag).
            if let Some(win) = app.get_webview_window("main") {
                if !launched_hidden() {
                    let _ = win.show();
                    let _ = win.set_focus();
                }

                // Closing the window hides it to the tray so the app stays
                // always-on (the agent can still ring it). Quit via the tray menu.
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

/// Build the system-tray / menu-bar icon with Show, Start at Login, and Quit actions.
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let start_at_login = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at Login",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &start_at_login, &quit])?;

    // Keep a handle so the menu event can reflect the actual OS state back into the
    // checkmark after toggling.
    let autostart_item = start_at_login.clone();

    let mut builder = TrayIconBuilder::new()
        .tooltip("Agent Voice App")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => reveal_main(app),
            "autostart" => toggle_autostart(app, &autostart_item),
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

/// Flip the OS autostart registration and sync the tray checkmark to whatever the
/// OS actually reports afterwards (so a failed toggle doesn't lie in the menu).
fn toggle_autostart(app: &tauri::AppHandle, item: &CheckMenuItem<tauri::Wry>) {
    let manager = app.autolaunch();
    let currently_enabled = manager.is_enabled().unwrap_or(false);
    let result = if currently_enabled {
        manager.disable()
    } else {
        manager.enable()
    };
    if let Err(e) = result {
        eprintln!("voice-call: failed to toggle autostart: {e}");
    }
    let actual = manager.is_enabled().unwrap_or(currently_enabled);
    let _ = item.set_checked(actual);
}

/// Enable autostart the first time the app runs. Guarded by a marker file so this
/// default is applied only once — later the user's tray choice is authoritative.
/// Best-effort: never blocks startup.
fn ensure_autostart_default(app: &tauri::AppHandle) {
    let Ok(dir) = paths::app_data_dir() else {
        return;
    };
    if !claim_first_run(&dir.join(".autostart-initialized")) {
        return;
    }
    if let Err(e) = app.autolaunch().enable() {
        eprintln!("voice-call: failed to enable autostart on first run: {e}");
    }
}

/// Returns `true` exactly once per install: `true` when the marker doesn't yet
/// exist (creating it as a side effect), `false` on every subsequent call. If the
/// marker can't be created we still return `true` but don't loop forever because
/// callers only act on first run — a write failure just means the default may be
/// retried next launch, which is harmless.
fn claim_first_run(marker: &Path) -> bool {
    if marker.exists() {
        return false;
    }
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker, b"1");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_marker() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("voice-autostart-test-{}-{n}", std::process::id()));
        p.push(".autostart-initialized");
        p
    }

    #[test]
    fn claim_first_run_is_true_only_once() {
        let marker = temp_marker();
        assert!(!marker.exists());

        // First call claims the first run and creates the marker.
        assert!(claim_first_run(&marker));
        assert!(marker.exists());

        // Every subsequent call is a no-op returning false.
        assert!(!claim_first_run(&marker));
        assert!(!claim_first_run(&marker));

        let _ = std::fs::remove_dir_all(marker.parent().unwrap());
    }
}
