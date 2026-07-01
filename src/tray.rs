#[cfg(target_os = "linux")]
mod platform {
    use std::{path::PathBuf, sync::mpsc::Sender, thread};

    use eframe::egui;
    use ksni::{blocking::TrayMethods, menu::StandardItem, Icon, MenuItem, ToolTip, Tray};

    use crate::events::AppEvent;

    const APP_ID: &str = "com.visorcraft.realistic-mouse-jiggler";
    const APP_TITLE: &str = "Realistic Mouse Jiggler";
    const FALLBACK_ICON_NAME: &str = "preferences-desktop-mouse";
    const TRAY_ICON_SVG: &str = include_str!("../assets/tray-icon.svg");
    const TRAY_ICON_SIZES: [usize; 6] = [16, 22, 24, 32, 48, 64];

    pub struct TrayState {
        _handle: ksni::blocking::Handle<LinuxTray>,
        _icon_path: Option<PathBuf>,
    }

    impl TrayState {
        pub fn new(tx: Sender<AppEvent>, egui_ctx: egui::Context) -> anyhow::Result<Self> {
            let icon_path = install_runtime_icon();
            let icon_name = icon_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| FALLBACK_ICON_NAME.to_string());
            let tray = LinuxTray {
                tx,
                egui_ctx,
                icon_name,
            };
            let handle = tray
                .spawn()
                .map_err(|error| anyhow::anyhow!("could not create Linux SNI tray: {error}"))?;
            Ok(Self {
                _handle: handle,
                _icon_path: icon_path,
            })
        }
    }

    struct LinuxTray {
        tx: Sender<AppEvent>,
        egui_ctx: egui::Context,
        icon_name: String,
    }

    impl LinuxTray {
        fn send(&self, event: AppEvent) {
            let _ = self.tx.send(event);
            self.egui_ctx.request_repaint();
        }

        fn open(&self) {
            thread::spawn(crate::app::restore_plasma_window);
            self.send(AppEvent::ShowWindow);
        }
    }

    impl Tray for LinuxTray {
        fn id(&self) -> String {
            APP_ID.to_string()
        }

        fn title(&self) -> String {
            APP_TITLE.to_string()
        }

        fn icon_name(&self) -> String {
            self.icon_name.clone()
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            TRAY_ICON_SIZES.into_iter().map(tray_icon).collect()
        }

        fn tool_tip(&self) -> ToolTip {
            ToolTip {
                icon_name: self.icon_name(),
                icon_pixmap: vec![tray_icon(32)],
                title: APP_TITLE.to_string(),
                description: "Click to open controls.".to_string(),
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.open();
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                open_item(),
                item("Start", AppEvent::StartRequested),
                item("Stop", AppEvent::StopRequested),
                MenuItem::Separator,
                item("Quit", AppEvent::QuitRequested),
            ]
        }
    }

    fn open_item() -> MenuItem<LinuxTray> {
        StandardItem {
            label: "Open".to_string(),
            activate: Box::new(|tray: &mut LinuxTray| tray.open()),
            ..Default::default()
        }
        .into()
    }

    fn item(label: &str, event: AppEvent) -> MenuItem<LinuxTray> {
        StandardItem {
            label: label.to_string(),
            activate: Box::new(move |tray: &mut LinuxTray| tray.send(event.clone())),
            ..Default::default()
        }
        .into()
    }

    fn install_runtime_icon() -> Option<PathBuf> {
        let base_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let icon_dir = base_dir.join("realistic-mouse-jiggler");
        let icon_path = icon_dir.join("tray-icon.svg");

        std::fs::create_dir_all(&icon_dir).ok()?;
        std::fs::write(&icon_path, TRAY_ICON_SVG).ok()?;

        Some(icon_path)
    }

    fn tray_icon(size: usize) -> Icon {
        let width = size;
        let height = size;
        let mut argb = vec![0; width * height * 4];

        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                let nx = (x as f32 + 0.5) / width as f32;
                let ny = (y as f32 + 0.5) / height as f32;
                let body = ((nx - 0.5) / 0.34).powi(2) + ((ny - 0.53) / 0.43).powi(2);

                if body <= 1.0 {
                    set_argb(&mut argb, index, 255, 248, 250, 252);
                }

                if (0.78..=1.0).contains(&body) {
                    set_argb(&mut argb, index, 255, 23, 32, 42);
                }

                if (0.48..=0.52).contains(&nx) && (0.13..=0.43).contains(&ny) {
                    set_argb(&mut argb, index, 255, 23, 32, 42);
                }
            }
        }

        draw_rounded_rect(&mut argb, width, height, 0.44, 0.21, 0.12, 0.21);
        draw_jiggle(&mut argb, width, height);

        Icon {
            width: width as i32,
            height: height as i32,
            data: argb,
        }
    }

    fn set_argb(data: &mut [u8], index: usize, a: u8, r: u8, g: u8, b: u8) {
        data[index] = a;
        data[index + 1] = r;
        data[index + 2] = g;
        data[index + 3] = b;
    }

    fn draw_rounded_rect(
        data: &mut [u8],
        width: usize,
        height: usize,
        x0: f32,
        y0: f32,
        w: f32,
        h: f32,
    ) {
        let radius = w / 2.0;
        for y in 0..height {
            for x in 0..width {
                let nx = (x as f32 + 0.5) / width as f32;
                let ny = (y as f32 + 0.5) / height as f32;
                let clamped_x = nx.clamp(x0 + radius, x0 + w - radius);
                let clamped_y = ny.clamp(y0 + radius, y0 + h - radius);
                let distance = ((nx - clamped_x).powi(2) + (ny - clamped_y).powi(2)).sqrt();

                if distance <= radius && (x0..=x0 + w).contains(&nx) && (y0..=y0 + h).contains(&ny)
                {
                    let index = (y * width + x) * 4;
                    set_argb(data, index, 255, 116, 221, 167);
                }
            }
        }
    }

    fn draw_jiggle(data: &mut [u8], width: usize, height: usize) {
        let thickness = (width as f32 * 0.055).max(1.0);
        for x in (width / 5)..(width - width / 5) {
            let progress = x as f32 / (width - 1) as f32;
            let center_y =
                height as f32 * (0.63 + 0.055 * (progress * std::f32::consts::TAU * 3.0).sin());
            draw_dot(data, width, height, x as f32, center_y, thickness);
        }
    }

    fn draw_dot(
        data: &mut [u8],
        width: usize,
        height: usize,
        center_x: f32,
        center_y: f32,
        radius: f32,
    ) {
        let min_x = (center_x - radius).floor().max(0.0) as usize;
        let max_x = (center_x + radius).ceil().min((width - 1) as f32) as usize;
        let min_y = (center_y - radius).floor().max(0.0) as usize;
        let max_y = (center_y + radius).ceil().min((height - 1) as f32) as usize;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let distance =
                    ((x as f32 - center_x).powi(2) + (y as f32 - center_y).powi(2)).sqrt();
                if distance <= radius {
                    let index = (y * width + x) * 4;
                    set_argb(data, index, 255, 48, 209, 88);
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use std::sync::mpsc::Sender;

    use eframe::egui;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
        Icon, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    use crate::events::AppEvent;

    const OPEN_ID: &str = "open";
    const START_ID: &str = "start";
    const STOP_ID: &str = "stop";
    const QUIT_ID: &str = "quit";

    pub struct TrayState {
        _tray_icon: TrayIcon,
    }

    impl TrayState {
        pub fn new(tx: Sender<AppEvent>, egui_ctx: egui::Context) -> anyhow::Result<Self> {
            install_menu_handler(tx.clone(), egui_ctx.clone());
            install_tray_handler(tx, egui_ctx);

            let menu = Menu::new();
            let open = MenuItem::with_id(MenuId::new(OPEN_ID), "Open", true, None);
            let start = MenuItem::with_id(MenuId::new(START_ID), "Start", true, None);
            let stop = MenuItem::with_id(MenuId::new(STOP_ID), "Stop", true, None);
            let quit = MenuItem::with_id(MenuId::new(QUIT_ID), "Quit", true, None);
            let separator = PredefinedMenuItem::separator();
            menu.append_items(&[&open, &start, &stop, &separator, &quit])?;

            let tray_icon = TrayIconBuilder::new()
                .with_tooltip("Realistic Mouse Jiggler")
                .with_icon(tray_icon())
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(true)
                .with_menu_on_right_click(true)
                .build()?;

            Ok(Self {
                _tray_icon: tray_icon,
            })
        }
    }

    fn install_menu_handler(tx: Sender<AppEvent>, egui_ctx: egui::Context) {
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let message = match event.id.as_ref() {
                OPEN_ID => Some(AppEvent::ShowWindow),
                START_ID => Some(AppEvent::StartRequested),
                STOP_ID => Some(AppEvent::StopRequested),
                QUIT_ID => Some(AppEvent::QuitRequested),
                _ => None,
            };

            if let Some(message) = message {
                let _ = tx.send(message);
                egui_ctx.request_repaint();
            }
        }));
    }

    fn install_tray_handler(tx: Sender<AppEvent>, egui_ctx: egui::Context) {
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick { .. }
            ) {
                let _ = tx.send(AppEvent::ShowWindow);
                egui_ctx.request_repaint();
            }
        }));
    }

    fn tray_icon() -> Icon {
        let width = 32;
        let height = 32;
        let mut rgba = vec![0; width * height * 4];

        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                let cx = x as f32 - 16.0;
                let cy = y as f32 - 16.0;
                let radius = (cx * cx + cy * cy).sqrt();

                if radius <= 14.0 {
                    rgba[index] = 29;
                    rgba[index + 1] = 41;
                    rgba[index + 2] = 57;
                    rgba[index + 3] = 255;
                }

                if (10..=21).contains(&x) && (14..=18).contains(&y) {
                    rgba[index] = 117;
                    rgba[index + 1] = 221;
                    rgba[index + 2] = 167;
                    rgba[index + 3] = 255;
                }

                if ((x == 7 || x == 8) && (15..=17).contains(&y))
                    || ((x == 23 || x == 24) && (15..=17).contains(&y))
                    || ((x == 9 || x == 22) && y == 16)
                {
                    rgba[index] = 117;
                    rgba[index + 1] = 221;
                    rgba[index + 2] = 167;
                    rgba[index + 3] = 255;
                }
            }
        }

        Icon::from_rgba(rgba, width as u32, height as u32)
            .expect("generated tray icon should be valid")
    }
}

pub use platform::TrayState;
