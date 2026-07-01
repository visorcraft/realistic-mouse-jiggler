#[cfg(target_os = "linux")]
mod platform {
    use std::sync::mpsc::Sender;

    use eframe::egui;
    use ksni::{blocking::TrayMethods, menu::StandardItem, Icon, MenuItem, Tray};

    use crate::events::AppEvent;

    pub struct TrayState {
        _handle: ksni::blocking::Handle<LinuxTray>,
    }

    impl TrayState {
        pub fn new(tx: Sender<AppEvent>, egui_ctx: egui::Context) -> anyhow::Result<Self> {
            let tray = LinuxTray { tx, egui_ctx };
            let handle = tray
                .spawn()
                .map_err(|error| anyhow::anyhow!("could not create Linux SNI tray: {error}"))?;
            Ok(Self { _handle: handle })
        }
    }

    struct LinuxTray {
        tx: Sender<AppEvent>,
        egui_ctx: egui::Context,
    }

    impl LinuxTray {
        fn send(&self, event: AppEvent) {
            let _ = self.tx.send(event);
            self.egui_ctx.request_repaint();
        }
    }

    impl Tray for LinuxTray {
        fn id(&self) -> String {
            "realistic-mouse-jiggler".to_string()
        }

        fn title(&self) -> String {
            "Realistic Mouse Jiggler".to_string()
        }

        fn icon_name(&self) -> String {
            "input-mouse".to_string()
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            vec![tray_icon()]
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(AppEvent::ShowWindow);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                item("Open", AppEvent::ShowWindow),
                item("Start", AppEvent::StartRequested),
                item("Stop", AppEvent::StopRequested),
                MenuItem::Separator,
                item("Quit", AppEvent::QuitRequested),
            ]
        }
    }

    fn item(label: &str, event: AppEvent) -> MenuItem<LinuxTray> {
        StandardItem {
            label: label.to_string(),
            activate: Box::new(move |tray: &mut LinuxTray| tray.send(event.clone())),
            ..Default::default()
        }
        .into()
    }

    fn tray_icon() -> Icon {
        let width = 32;
        let height = 32;
        let mut argb = vec![0; width * height * 4];

        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                let cx = x as f32 - 16.0;
                let cy = y as f32 - 16.0;
                let radius = (cx * cx + cy * cy).sqrt();

                if radius <= 14.0 {
                    set_argb(&mut argb, index, 255, 29, 41, 57);
                }

                if (10..=21).contains(&x) && (14..=18).contains(&y) {
                    set_argb(&mut argb, index, 255, 117, 221, 167);
                }

                if ((x == 7 || x == 8) && (15..=17).contains(&y))
                    || ((x == 23 || x == 24) && (15..=17).contains(&y))
                    || ((x == 9 || x == 22) && y == 16)
                {
                    set_argb(&mut argb, index, 255, 117, 221, 167);
                }
            }
        }

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
