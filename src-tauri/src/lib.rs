mod commands;
pub mod desktop;
mod domain;
mod error;
mod planner;
mod store;
mod window;

use commands::AppRuntime;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(error) = window::ensure_process_unelevated() {
        window::show_startup_error(&error.message);
        return;
    }
    window::enable_per_monitor_v2_dpi();
    let runtime = AppRuntime::initialize().expect("failed to initialize WorkSets data store");
    tauri::Builder::default()
        .manage(Arc::new(runtime))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let _ = app.emit(
                            "worksets-global-shortcut",
                            serde_json::json!({ "shortcut": shortcut.to_string() }),
                        );
                    }
                })
                .build(),
        )
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--autostart")
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let runtime = app.state::<Arc<AppRuntime>>();
            let mut settings = runtime.store.load_settings()?;
            for preset in runtime.store.list_presets()? {
                if let Some(hotkey) = preset.hotkey {
                    settings.hotkeys.insert(preset.id, hotkey);
                }
            }
            for shortcut in
                std::iter::once(&settings.capture_hotkey).chain(settings.hotkeys.values())
            {
                if let Err(error) = app.global_shortcut().register(shortcut.as_str()) {
                    eprintln!("Could not register shortcut '{shortcut}': {error}");
                }
            }
            setup_tray(app)?;
            if std::env::args().any(|argument| argument == "--autostart") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let minimize = window
                    .state::<Arc<AppRuntime>>()
                    .store
                    .load_settings()
                    .map(|settings| {
                        settings.minimize_to_tray
                            && settings.close_behavior == domain::CloseBehavior::Tray
                    })
                    .unwrap_or(true);
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::list_presets,
            commands::capture_workspace,
            commands::save_preset,
            commands::delete_preset,
            commands::plan_preset_apply,
            commands::apply_preset,
            commands::cancel_apply,
            commands::get_settings,
            commands::save_settings,
            commands::get_compatibility,
            commands::get_diagnostics,
            commands::check_for_updates,
            commands::export_preset,
            commands::import_preset,
            commands::get_preset_schema,
        ])
        .run(tauri::generate_context!())
        .expect("error while running WorkSets");
}

fn tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let runtime = app.state::<Arc<AppRuntime>>();
    let german = runtime
        .store
        .load_settings()
        .map(|settings| settings.language == "de")
        .unwrap_or(true);
    let busy = runtime.is_busy();
    let show_label = if german {
        "WorkSets öffnen"
    } else {
        "Open WorkSets"
    };
    let quit_label = if german { "Beenden" } else { "Quit" };
    let show = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    menu.append(&show)?;
    if busy {
        let busy_label = if german {
            "Wechsel läuft…"
        } else {
            "Switching…"
        };
        let busy_item = MenuItem::with_id(app, "busy", busy_label, false, None::<&str>)?;
        menu.append(&busy_item)?;
    }

    let summaries = runtime.store.list_summaries().unwrap_or_default();
    if !summaries.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        for preset in summaries {
            let item = MenuItem::with_id(
                app,
                format!("apply:{}", preset.id),
                preset.name,
                !busy,
                None::<&str>,
            )?;
            menu.append(&item)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = tray_menu(app.handle())?;

    let mut tray = TrayIconBuilder::with_id("worksets")
        .menu(&menu)
        .tooltip("WorkSets by Tracht Digital Labs")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "show" => show_main_window(app),
                "quit" => app.exit(0),
                value if value.starts_with("apply:") => {
                    let _ = app.emit(
                        "worksets-tray-apply",
                        serde_json::json!({ "presetId": value.trim_start_matches("apply:") }),
                    );
                }
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub(crate) fn refresh_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("worksets") {
        tray.set_menu(Some(tray_menu(app)?))?;
    }
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
