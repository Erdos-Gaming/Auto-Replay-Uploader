#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::{Arc, Mutex}, thread};

use config::Config;
use crossbeam_channel::{bounded, Receiver};
use muda::{Menu, MenuItem, MenuEvent, PredefinedMenuItem};
use std::time::Duration;
use tray_icon::{TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;


mod watcher;
mod uploader;
mod auth;
mod config;
mod icon;
mod startup;

const ID_TOGGLE_DELETE: &str = "toggle_delete";
const ID_TOGGLE_STARTUP: &str = "toggle_startup";
const ID_EXIT: &str = "exit";

struct App {
    config: Arc<Mutex<Config>>,
    toggle_delete_item: MenuItem,
    toggle_startup_item: MenuItem,
    _tray: tray_icon::TrayIcon,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // We never create windows, so nothing to do here.
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
        // No windows — this will never fire.
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Process any pending tray icon events (left-click etc.)
        while TrayIconEvent::receiver().try_recv().is_ok() {
            // Right-click menu pops up automatically; nothing extra needed.
        }

        // Process context menu clicks
        while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            match menu_event.id().0.as_str() {
                ID_EXIT => {
                    log::info!("Exit requested via tray menu.");
                    event_loop.exit();
                }
                ID_TOGGLE_DELETE => {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.delete_after_upload = !cfg.delete_after_upload;
                    let new_val = cfg.delete_after_upload;
                    log::info!("delete_after_upload toggled → {}", new_val);

                    self.toggle_delete_item.set_text(delete_label(new_val));

                    if let Err(e) = cfg.save() {
                        log::error!("Failed to save config: {:#?}", e);
                    }
                }
                ID_TOGGLE_STARTUP => {
                    match startup::toggle() {
                        Ok(enabled) => {
                            self.toggle_startup_item.set_text(startup_label(enabled));
                            log::info!("Launch at startup → {}", enabled);
                        }
                        Err(e) => log::error!("Failed to toggle startup: {:#?}", e),
                    }
                }
                _ => {}
            }
        }

        // Sleep until the next OS message — zero CPU spin.
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}


fn main() {
    // Initialise logger (stderr in debug, log file in release)
    init_logger();

    // Load config — creates a default config.toml on first run
    let config = Arc::new(Mutex::new(match Config::load() {
        Ok(c) => {
            log::info!("Config loaded from {}", Config::config_path().display());
            c
        }
        Err(e) => {
            log::error!("Failed to load config: {:#?}. Using defaults.", e);
            Config::default()
        }
    }));

    // Ensure all watch directories exist
    {
        let cfg = config.lock().unwrap();
        for dir in &cfg.watch_dirs {
            if let Err(e) = std::fs::create_dir_all(dir) {
                log::warn!("Could not create watch_dir '{}': {}", dir, e);
            }
        }
    }
    
    let discord_user_id = match auth::ensure_user_id(config.clone(), std::time::Duration::from_secs(300)) {
        Ok(id) => id,
        Err(e) => {
            log::error!("Discord authorization failed: {:#?}", e);
            std::process::exit(1);
        }
    };
    log::info!("Authorised as Discord user {}", discord_user_id);

    let (file_tx, file_rx) = bounded::<PathBuf>(64);

    let _watcher = {
        let dirs = config.lock().unwrap().watch_dirs.clone();
        if let Err(e) = watcher::start_watcher(config.clone(), &dirs, file_tx) {
            log::error!("Failed to start watcher: {:#?}", e);
            std::process::exit(1);
        }
    };

    spawn_upload_worker(file_rx, Arc::clone(&config), discord_user_id);


    let menu = Menu::new();

    let status_item = MenuItem::with_id("status", concat!("Erdos Auto Uploader v", env!("CARGO_PKG_VERSION")), false, None);
    let sep1 = PredefinedMenuItem::separator();

    let delete_enabled = config.lock().unwrap().delete_after_upload;
    let toggle_delete_item =
        MenuItem::with_id(ID_TOGGLE_DELETE, delete_label(delete_enabled), true, None);

    let startup_enabled = startup::is_enabled();
    let toggle_startup_item =
        MenuItem::with_id(ID_TOGGLE_STARTUP, startup_label(startup_enabled), true, None);

    let sep2 = PredefinedMenuItem::separator();
    let exit_item = MenuItem::with_id(ID_EXIT, "Exit", true, None);

    menu.append_items(&[&status_item, &sep1, &toggle_delete_item, &toggle_startup_item, &sep2, &exit_item])
        .expect("Failed to build tray menu");

    let tray_icon = icon::load_tray_icon();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(concat!("Erdos Auto Uploader v", env!("CARGO_PKG_VERSION")))
        .with_icon(tray_icon)
        .build()
        .expect("Failed to create tray icon");

    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let mut app = App {
        config,
        toggle_delete_item,
        toggle_startup_item,
        _tray: tray,
    };

    event_loop.run_app(&mut app).expect("Event loop error");
}

fn spawn_upload_worker(rx: Receiver<PathBuf>, config: Arc<Mutex<Config>>, discord_user_id: u64) {
    thread::Builder::new()
        .name("upload-worker".into())
        .spawn(move || {
            // Simple per-path debounce: skip if same path fires again within 1 s
            let mut last_path: Option<PathBuf> = None;
            let mut last_time = std::time::Instant::now();

            loop {
                match rx.recv() {
                    Ok(path) => {
                        let now = std::time::Instant::now();
                        if last_path.as_deref() == Some(path.as_path())
                            && now.duration_since(last_time) < Duration::from_secs(1)
                        {
                            continue;
                        }
                        last_path = Some(path.clone());
                        last_time = now;

                        // Wait for the writer to finish before we try to read
                        let settle_ms = config.lock().unwrap().settle_ms;
                        if settle_ms > 0 {
                            thread::sleep(Duration::from_millis(settle_ms));
                        }

                        // Snapshot config so we don't hold the lock during I/O
                        let cfg = config.lock().unwrap().clone();

                        log::info!("New file detected: {}", path.display());

                        match uploader::upload_file(&path, &cfg, discord_user_id) {
                            Ok(()) => {
                                log::info!("Upload complete: {}", path.display());
                                if cfg.delete_after_upload {
                                    if let Err(e) = std::fs::remove_file(&path) {
                                        log::error!("Delete failed: {:#?}", e);
                                    } else {
                                        log::info!("Deleted: {}", path.display());
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!(
                                    "Upload failed for {}: {:#?}",
                                    path.display(),
                                    e
                                );
                            }
                        }
                    }
                    Err(_) => {
                        log::info!("File channel closed — upload worker exiting.");
                        break;
                    }
                }
            }
        })
        .expect("Failed to spawn upload worker thread");
}

fn delete_label(enabled: bool) -> &'static str {
    if enabled {
        "✓  Delete after upload"
    } else {
        "    Delete after upload"
    }
}

fn startup_label(enabled: bool) -> &'static str {
    if enabled {
        "✓  Launch at startup"
    } else {
        "    Launch at startup"
    }
}


fn init_logger() {
    #[cfg(debug_assertions)]
    {
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info"),
        )
        .init();
    }

    #[cfg(not(debug_assertions))]
    {
        use log4rs::{
            append::rolling_file::{
                policy::compound::{
                    trigger::size::SizeTrigger,
                    roll::fixed_window::FixedWindowRoller,
                    roll::Roll,
                    CompoundPolicy
                },
                RollingFileAppender,
            },
            encode::pattern::PatternEncoder,
        };
        let log_dir = config::app_data_dir();
        let log_path = log_dir.join("erdos.log");
        let archive_pattern = log_dir.join("erdos-{}.log").to_string_lossy().to_string();
        let roller = FixedWindowRoller::builder()
            .build(&archive_pattern, 7)
            .expect("Cannot build log roller");
        if log_path.exists() {
            roller.roll(&log_path).expect("Cannot roll log on startup");
        }
        let trigger = SizeTrigger::new(u64::MAX);
        let policy = CompoundPolicy::new(Box::new(trigger), Box::new(roller));
        let appender = RollingFileAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} {l} {t} - {m}{n}")))
            .build(&log_path, Box::new(policy))
            .expect("Cannot build log appender");
        let log_config = log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder().build("file", Box::new(appender)))
            .build(
                log4rs::config::Root::builder()
                    .appender("file")
                    .build(log::LevelFilter::Info),
            )
            .expect("Cannot build log config");
        
        log4rs::init_config(log_config).expect("Cannot init logger");
        log::info!("Logging to {}", log_path.display());
    }
}
