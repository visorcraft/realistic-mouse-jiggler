#[cfg(target_os = "linux")]
mod platform {
    use std::{path::PathBuf, sync::mpsc::Sender, thread};

    use eframe::egui;
    use ksni::{blocking::TrayMethods, menu::StandardItem, Icon, MenuItem, ToolTip, Tray};

    use crate::{events::AppEvent, icons};

    const APP_ID: &str = "com.visorcraft.realistic-mouse-jiggler";
    const APP_TITLE: &str = "Realistic Mouse Jiggler";
    const FALLBACK_ICON_NAME: &str = "preferences-desktop-mouse";
    const TRAY_ICON_PNGS: [&[u8]; 6] = [
        icons::RMJ_16_PNG,
        icons::RMJ_24_PNG,
        icons::RMJ_32_PNG,
        icons::RMJ_48_PNG,
        icons::RMJ_64_PNG,
        icons::RMJ_128_PNG,
    ];

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
            TRAY_ICON_PNGS.into_iter().filter_map(tray_icon).collect()
        }

        fn tool_tip(&self) -> ToolTip {
            ToolTip {
                icon_name: self.icon_name(),
                icon_pixmap: tray_icon(icons::RMJ_32_PNG).into_iter().collect(),
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
        let icon_path = icon_dir.join("rmj.png");

        std::fs::create_dir_all(&icon_dir).ok()?;
        std::fs::write(&icon_path, icons::RMJ_64_PNG).ok()?;

        Some(icon_path)
    }

    fn tray_icon(bytes: &[u8]) -> Option<Icon> {
        let icon = icons::decode_png(bytes).ok()?;
        let mut argb = Vec::with_capacity(icon.rgba.len());

        for pixel in icon.rgba.chunks_exact(4) {
            argb.extend_from_slice(&[pixel[3], pixel[0], pixel[1], pixel[2]]);
        }

        Some(Icon {
            width: icon.width as i32,
            height: icon.height as i32,
            data: argb,
        })
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

    use crate::{events::AppEvent, icons};

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
        let icon =
            icons::decode_png(icons::RMJ_32_PNG).expect("embedded RMJ tray icon should be valid");
        Icon::from_rgba(icon.rgba, icon.width, icon.height)
            .expect("embedded RMJ tray icon should be valid")
    }
}

pub use platform::TrayState;
