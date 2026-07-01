#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod events;
mod icons;
mod input;
mod jiggler;
mod tray;

use std::sync::{atomic::AtomicBool, mpsc, Arc, Mutex, RwLock};

use eframe::egui;

use app::MouseJigglerApp;
use input::SharedCaptureTarget;

fn main() -> eframe::Result<()> {
    let config_path = config::config_path();
    let config = Arc::new(RwLock::new(config::load_config(&config_path)));
    let running = Arc::new(AtomicBool::new(false));
    let capture_target: SharedCaptureTarget = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel();

    input::spawn_input_listener(
        tx.clone(),
        Arc::clone(&running),
        Arc::clone(&config),
        Arc::clone(&capture_target),
    );
    jiggler::spawn_jiggler(tx.clone(), Arc::clone(&running), Arc::clone(&config));
    #[cfg(target_os = "linux")]
    icons::install_linux_desktop_icon();

    let viewport = egui::ViewportBuilder::default()
        .with_app_id("com.visorcraft.realistic-mouse-jiggler")
        .with_title("Realistic Mouse Jiggler")
        .with_inner_size([440.0, 310.0])
        .with_min_inner_size([420.0, 300.0])
        .with_resizable(false)
        .with_icon(icons::window_icon());

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Realistic Mouse Jiggler",
        native_options,
        Box::new(move |cc| {
            app::configure_ui(&cc.egui_ctx);
            Ok(Box::new(MouseJigglerApp::new(
                tx,
                rx,
                config,
                config_path,
                running,
                capture_target,
            )))
        }),
    )
}
