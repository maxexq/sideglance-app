//! Sideglance desktop app: a tray icon, the control page in a window, and the local server
//! (from sideglance-core) that talks to the Ditoo Pro and answers Claude Code hooks.
//!
//! Started at sign-in with --hidden, it sits in the tray without opening the window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use rust_embed::RustEmbed;
use sideglance_core::config::Config;
use sideglance_core::device::{Device, Divoom, Fake};
use sideglance_core::http;
use sideglance_core::state::{self, App};
use sideglance_core::update::{self, Available, Updater};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_updater::UpdaterExt;

/// The built control page (ui/dist), inside the app.
#[derive(RustEmbed)]
#[folder = "../ui/dist"]
struct Ui;

const HIDDEN: &str = "--hidden";

fn assets() -> http::Assets {
    Arc::new(|path: &str| {
        Ui::get(path).map(|file| (file.data.into_owned(), file.metadata.mimetype().to_string()))
    })
}

/// Checks GitHub Releases (latest.json) and installs signed updates.
struct AppUpdater(AppHandle);

impl Updater for AppUpdater {
    fn check(&self) -> Result<Option<Available>, String> {
        let updater = self.0.updater().map_err(|e| e.to_string())?;
        let found = tauri::async_runtime::block_on(updater.check()).map_err(|e| e.to_string())?;
        Ok(found.map(|u| Available { version: u.version.clone(), notes: u.body.clone().unwrap_or_default() }))
    }

    fn install(&self) -> Result<(), String> {
        let updater = self.0.updater().map_err(|e| e.to_string())?;
        let found = tauri::async_runtime::block_on(updater.check()).map_err(|e| e.to_string())?;
        let Some(update) = found else {
            return Err("This is already the newest version.".into());
        };
        tauri::async_runtime::block_on(update.download_and_install(|_, _| {}, || {}))
            .map_err(|e| e.to_string())?;
        // On Windows the installer has already closed the app; on macOS the new app is in place.
        self.0.restart()
    }
}

/// Checks when the app starts and every few hours, and keeps the tray item's text in step
/// with what the page or the tray found.
fn watch_updates(core: Arc<App>, item: MenuItem<tauri::Wry>) {
    std::thread::spawn(move || {
        let mut last_check: Option<Instant> = None;
        let mut shown = String::new();
        loop {
            if last_check.is_none_or(|t| t.elapsed() >= update::CHECK_EVERY) {
                last_check = Some(Instant::now());
                let _ = core.updates.check();
            }
            let text = update_text(&core);
            if text != shown {
                let _ = item.set_text(&text);
                shown = text;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

fn update_text(core: &App) -> String {
    match core.updates.available_version() {
        Some(v) => format!("Update to {v} and restart"),
        None => "Check for updates".into(),
    }
}

/// Show the control page, creating its window the first time.
fn open_controls(handle: &AppHandle) {
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    // The page is bundled in the app, so the window shows it even before the local server is up;
    // it then talks to the server on 127.0.0.1 and reconnects by itself.
    let built = WebviewWindowBuilder::new(handle, "main", WebviewUrl::App("index.html".into()))
        .title("Sideglance")
        .inner_size(1180.0, 860.0)
        .min_inner_size(380.0, 600.0)
        // links to other sites (Open-Meteo, Windows Settings) open outside the app, so the
        // window always keeps the controls
        .on_navigation(|url| {
            let ours = matches!(url.scheme(), "tauri" | "asset")
                || matches!(url.host_str(), Some("tauri.localhost" | "localhost" | "127.0.0.1"));
            if !ours {
                let _ = open::that_detached(url.as_str());
            }
            ours
        })
        .build();
    if let Ok(window) = built {
        let hide = window.clone();
        // closing the window keeps the app running in the tray
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = hide.hide();
            }
        });
    }
}

fn main() {
    let config = Arc::new(Config::default_location());
    let device: Box<dyn Device> = if std::env::var("SIDEGLANCE_FAKE").as_deref() == Ok("1") {
        Box::new(Fake::default())
    } else {
        let c = Arc::clone(&config);
        Box::new(Divoom::new(Box::new(move || c.string("port"))))
    };
    let core = App::new(device, Arc::clone(&config), Box::new(state::fetch_usage));

    // the local server: the page, the API for Claude Code hooks, and /api/scene
    {
        let core = Arc::clone(&core);
        std::thread::spawn(move || {
            if let Err(e) = http::serve(core, assets(), false) {
                eprintln!("{e}");
            }
        });
    }
    // show what was chosen last time, or just connect so the page can say whether the device was found
    {
        let core = Arc::clone(&core);
        std::thread::spawn(move || {
            match core.config.string("start_with").as_str() {
                "usage" => core.set_usage(true),
                "weather" => core.set_weather(true),
                "system" => core.set_system(true),
                _ => core.try_connect(),
            }
            core.watch_battery();
            core.watch_screen();
        });
    }

    let hidden = std::env::args().any(|a| a == HIDDEN);
    let tray_core = Arc::clone(&core);
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|handle, _args, _cwd| open_controls(handle)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![HIDDEN])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // tray only, no Dock icon

            let handle = app.handle();
            let starts_at_sign_in = handle.autolaunch().is_enabled().unwrap_or(false);
            let open = MenuItem::with_id(handle, "open", "Open controls", true, None::<&str>)?;
            let usage = MenuItem::with_id(handle, "usage", "Show AI usage", true, None::<&str>)?;
            let dismiss = MenuItem::with_id(handle, "dismiss", "Dismiss notification", true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(handle, "autostart", "Start at sign-in", true, starts_at_sign_in, None::<&str>)?;
            let updates = MenuItem::with_id(handle, "update", "Check for updates", true, None::<&str>)?;
            let quit = MenuItem::with_id(handle, "quit", "Quit Sideglance", true, None::<&str>)?;
            let menu = Menu::with_items(handle, &[
                &open, &PredefinedMenuItem::separator(handle)?, &usage, &dismiss,
                &PredefinedMenuItem::separator(handle)?, &autostart, &updates, &quit,
            ])?;
            tray_core.updates.set_updater(Arc::new(AppUpdater(handle.clone())));
            watch_updates(Arc::clone(&tray_core), updates.clone());
            let core = Arc::clone(&tray_core);
            let check = autostart.clone();
            TrayIconBuilder::with_id("main")
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .tooltip("Sideglance")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |handle, event| match event.id().as_ref() {
                    "open" => open_controls(handle),
                    "usage" => {
                        core.take_screen();
                        core.dismiss();
                        core.set_usage(true);
                        let _ = core.config.update(&[("start_with", "usage".into())]);
                    }
                    "dismiss" => {
                        core.dismiss();
                    }
                    "autostart" => {
                        let launcher = handle.autolaunch();
                        let on = launcher.is_enabled().unwrap_or(false);
                        let _ = if on { launcher.disable() } else { launcher.enable() };
                        let _ = check.set_checked(launcher.is_enabled().unwrap_or(!on));
                    }
                    "update" => {
                        if core.updates.available_version().is_some() {
                            core.install_update();
                        } else {
                            // the result shows on the page, and in this item's text if there is one
                            let core = Arc::clone(&core);
                            std::thread::spawn(move || { let _ = core.updates.check(); });
                            open_controls(handle);
                        }
                    }
                    "quit" => {
                        core.close_device();
                        handle.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        open_controls(tray.app_handle());
                    }
                })
                .build(app)?;

            // The first run turns on start at sign-in; the tray menu turns it off.
            let first_run = !core_first_run_done(&tray_core);
            if first_run {
                let _ = handle.autolaunch().enable();
                let _ = autostart.set_checked(true);
            }
            if !hidden {
                open_controls(handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Sideglance could not start")
        .run(|_handle, event| {
            // keep running in the tray when the last window closes
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// True after the first run; records it so the first run happens once.
fn core_first_run_done(core: &App) -> bool {
    if core.config.load().contains_key("first_run_done") {
        return true;
    }
    let _ = core.config.update(&[("first_run_done", true.into())]);
    false
}
