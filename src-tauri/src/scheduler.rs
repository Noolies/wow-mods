//! Background work: refresh the curated list, check for addon updates on start-up
//! and every few hours, install updates allowed to update on their own, and
//! notify when the window is hidden.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::ops;
use crate::providers::catalog;
use crate::state::AppState;

static LAST_NOTIFIED: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        {
            let state = app.state::<AppState>();
            match catalog::refresh(&state).await {
                Ok(true) => {
                    let _ = app.emit("catalog-changed", ());
                }
                Ok(false) => {}
                Err(error) => log::info!("Addon list not refreshed: {error:#}"),
            }
        }

        let mut last_run: Option<Instant> = None;
        loop {
            let settings = app.state::<AppState>().settings();
            let interval = Duration::from_secs(u64::from(settings.check_interval_hours.clamp(1, 48)) * 3600);
            let due = match last_run {
                None => settings.check_on_startup,
                Some(at) => settings.background_checks && at.elapsed() >= interval,
            };
            if due {
                run_cycle(&app, false).await;
                last_run = Some(Instant::now());
            } else if last_run.is_none() {
                last_run = Some(Instant::now());
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

/// Checks every game; installs automatic updates when the game isn't running.
/// `user_asked` (from the tray) always reports the outcome.
pub async fn run_cycle(app: &AppHandle, user_asked: bool) {
    let state = app.state::<AppState>();
    let settings = state.settings();
    for installation in state.installations() {
        if !installation.path.is_dir() {
            continue;
        }
        if let Err(error) = ops::check_updates(app, &state, &installation.id).await {
            log::warn!("Update check failed for {}: {error:#}", installation.label);
            continue;
        }
        let automatic = ops::pending_updates(&state, &installation.id, true);
        let mut installed = 0;
        if !automatic.is_empty() && !state.is_game_running(&installation) {
            match ops::update_addons(app, &state, &installation.id, &automatic).await {
                Ok(batch) => installed = batch.succeeded.len(),
                Err(error) => log::warn!("Automatic updates failed: {error:#}"),
            }
        }
        let waiting = ops::pending_updates(&state, &installation.id, false).len();

        let message = if installed > 0 {
            Some(format!(
                "Updated {installed} addon{} for {}.",
                if installed == 1 { "" } else { "s" },
                installation.label
            ))
        } else if waiting > 0 {
            Some(format!(
                "{waiting} addon update{} ready for {}.",
                if waiting == 1 { " is" } else { "s are" },
                installation.label
            ))
        } else if user_asked {
            Some(format!("Everything is up to date for {}.", installation.label))
        } else {
            None
        };

        let changed = {
            let mut guard = LAST_NOTIFIED.lock().unwrap();
            let map = guard.get_or_insert_with(HashMap::new);
            let previous = map.insert(installation.id.clone(), waiting + installed);
            previous != Some(waiting + installed) || installed > 0
        };
        if let Some(message) = message {
            let should_notify = settings.notifications && (user_asked || (changed && !window_visible(app)));
            if should_notify {
                let _ = app.notification().builder().title("Hearthforge").body(&message).show();
            }
        }
    }
}
