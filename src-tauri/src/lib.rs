//! Hearthforge: a lightweight addon manager and launcher, starting with WoW Forever.

mod backup;
mod commands;
mod error;
mod games;
mod http;
mod install;
mod model;
mod ops;
mod providers;
mod scheduler;
mod secrets;
mod share;
mod state;
mod store;
mod tray;

#[cfg(test)]
mod live_tests;

use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use crate::state::AppState;

pub fn run() {
    // reqwest is built without a bundled TLS provider; use ring, like the updater plugin.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--minimized"]),
            ))
            .plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("reqwest", log::LevelFilter::Warn)
                .level_for("hyper_util", log::LevelFilter::Warn)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: Some("hearthforge".into()) }),
                ])
                .max_file_size(2_000_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = AppState::init(&handle)?;
            app.manage(state);
            tray::create(&handle)?;
            let start_hidden = std::env::args().any(|arg| arg == "--minimized");
            if !start_hidden {
                tray::show_main(&handle);
            }
            scheduler::spawn(handle);
            log::info!("Hearthforge {} started", app.package_info().version);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window
                    .app_handle()
                    .try_state::<AppState>()
                    .map(|s| s.settings().close_to_tray)
                    .unwrap_or(false);
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::save_settings,
            commands::set_secret,
            commands::refresh_catalog,
            commands::open_folder,
            commands::reveal_addon,
            commands::detect_installations,
            commands::inspect_folder,
            commands::add_installations,
            commands::update_installation,
            commands::remove_installation,
            commands::select_installation,
            commands::game_running,
            commands::play,
            commands::list_addons,
            commands::cached_updates,
            commands::check_updates,
            commands::install_addon,
            commands::update_addons,
            commands::uninstall_addon,
            commands::set_addon_options,
            commands::link_addon,
            commands::link_all,
            commands::search_addons,
            commands::addon_details,
            commands::install_from_url,
            commands::install_from_file,
            commands::export_pack,
            commands::preview_pack,
            commands::import_pack,
            commands::list_backups,
            commands::create_backup,
            commands::restore_backup,
            commands::delete_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hearthforge");
}
