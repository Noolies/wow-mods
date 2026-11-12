//! System tray icon: open the window, play the last game, check for updates, quit.

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::AppState;

struct TrayItems {
    play: MenuItem<Wry>,
}

struct TrayState(Mutex<Option<TrayItems>>);

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The installation the tray's Play item starts: the last one used, else the first.
fn play_target(app: &AppHandle) -> Option<(String, String)> {
    let state = app.try_state::<AppState>()?;
    let settings = state.settings();
    let installations = state.installations();
    let chosen = settings
        .last_installation
        .as_deref()
        .and_then(|id| installations.iter().find(|i| i.id == id))
        .or_else(|| installations.first())?;
    Some((chosen.id.clone(), chosen.label.clone()))
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let target = play_target(app);
    let open = MenuItem::with_id(app, "open", "Open Hearthforge", true, None::<&str>)?;
    let play = MenuItem::with_id(
        app,
        "play",
        target.as_ref().map(|(_, label)| format!("Play {label}")).unwrap_or_else(|| "Play".to_string()),
        target.is_some(),
        None::<&str>,
    )?;
    let check = MenuItem::with_id(app, "check", "Check for addon updates", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Hearthforge", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &play, &check, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Hearthforge")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "play" => {
                if let Some((id, _)) = play_target(app) {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = handle.state::<AppState>();
                        if let Err(error) = crate::ops::play(&handle, &state, &id).await {
                            log::warn!("Tray launch failed: {error:#}");
                            let _ = handle.emit("toast", format!("{error:#}"));
                            show_main(&handle);
                        }
                    });
                }
            }
            "check" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::scheduler::run_cycle(&handle, true).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayState(Mutex::new(Some(TrayItems { play }))));
    Ok(())
}

/// Keeps the Play item in step with the selected game.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.try_state::<TrayState>() else { return };
    let guard = tray.0.lock().unwrap();
    let Some(items) = guard.as_ref() else { return };
    match play_target(app) {
        Some((_, label)) => {
            let _ = items.play.set_text(format!("Play {label}"));
            let _ = items.play.set_enabled(true);
        }
        None => {
            let _ = items.play.set_text("Play");
            let _ = items.play.set_enabled(false);
        }
    }
}
