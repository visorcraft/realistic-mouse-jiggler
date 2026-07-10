use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc, OnceLock, RwLock,
    },
};

use eframe::egui::{
    self, Align, Align2, Button, CentralPanel, Color32, Context, FontId, Frame, Label, Layout,
    RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, ViewportCommand,
};

#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

use crate::{
    config::{self, AppConfig, AppTheme, Binding, MovementDistance, MovementMode},
    events::AppEvent,
    icons,
    input::{BindTarget, CaptureRequest, SharedCaptureTarget},
    tray::TrayState,
};

const SIDEBAR_WIDTH: f32 = 246.0;
const ACTION_BUTTON_SIZE: egui::Vec2 = egui::vec2(116.0, 38.0);
const APP_LICENSE_TEXT: &str = include_str!("../LICENSE");
const THIRD_PARTY_LICENSES_TEXT: &str = include_str!("../docs/credits-third-party.md");
const CREDITS_TEXT: &str = include_str!("../CREDITS.md");
const RUNTIME_LICENSES_TEXT: &str = "Runtime components used by Realistic Mouse Jiggler\n\n\
Most runtime pieces are Rust crates listed in Third-party licenses. Platform services are used through system APIs:\n\n\
- Linux: StatusNotifierItem-compatible tray environments and optional ydotool for Wayland cursor movement.\n\
- macOS: Accessibility/Input Monitoring permissions and CoreGraphics APIs.\n\
- Windows: Win32 cursor and windowing APIs through windows-rs.\n\n\
Optional system tools keep their own upstream licenses and are provided by the operating system or package manager.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Dashboard,
    Settings,
    About,
    Licenses,
    Credits,
}

impl Page {
    fn about_group(self) -> bool {
        matches!(self, Self::About | Self::Licenses | Self::Credits)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LicenseDocument {
    App,
    ThirdParty,
    Credits,
    Runtime,
}

impl LicenseDocument {
    const ALL: [Self; 4] = [Self::App, Self::ThirdParty, Self::Credits, Self::Runtime];

    fn label(self) -> &'static str {
        match self {
            Self::App => "RMJ License",
            Self::ThirdParty => "Third-party",
            Self::Credits => "Acknowledgments",
            Self::Runtime => "Runtime components",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::App => "The complete MIT license text bundled into the application.",
            Self::ThirdParty => {
                "The cargo-about-generated bundle with every direct and transitive Rust crate, grouped by license text."
            }
            Self::Credits => "Narrative attribution for Realistic Mouse Jiggler and its integrations.",
            Self::Runtime => "System services and optional tools Realistic Mouse Jiggler integrates with.",
        }
    }

    fn body(self) -> &'static str {
        match self {
            Self::App => APP_LICENSE_TEXT,
            Self::ThirdParty => THIRD_PARTY_LICENSES_TEXT,
            Self::Credits => CREDITS_TEXT,
            Self::Runtime => RUNTIME_LICENSES_TEXT,
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    surface0: Color32,
    sidebar: Color32,
    surface1: Color32,
    surface2: Color32,
    text: Color32,
    muted: Color32,
    separator: Color32,
    separator_strong: Color32,
    accent: Color32,
    accent_hover: Color32,
    accent_mute: Color32,
    success: Color32,
    warning: Color32,
    error: Color32,
    is_dark: bool,
}

impl Palette {
    fn for_theme(theme: AppTheme, system_theme: Option<egui::Theme>) -> Self {
        match theme {
            AppTheme::System => match system_theme.unwrap_or(egui::Theme::Dark) {
                egui::Theme::Light => Self::light(),
                egui::Theme::Dark => Self::dark(),
            },
            AppTheme::Light => Self::light(),
            AppTheme::Dark => Self::dark(),
            AppTheme::OledBlack => Self::from_stops(
                rgb(0, 0, 0),
                rgb(5, 5, 5),
                rgb(17, 17, 17),
                rgb(245, 245, 245),
                rgb(45, 127, 249),
            ),
            AppTheme::GentleGecko => Self::from_stops(
                rgb(0, 0, 0),
                rgb(0, 51, 34),
                rgb(0, 89, 61),
                rgb(255, 255, 255),
                rgb(0, 184, 107),
            ),
            AppTheme::BlackKnight => Self::from_stops(
                rgb(0, 0, 0),
                rgb(0, 51, 102),
                rgb(0, 71, 143),
                rgb(255, 255, 255),
                rgb(0, 120, 212),
            ),
            AppTheme::Diamond => Self::from_stops(
                rgb(45, 91, 103),
                rgb(79, 127, 140),
                rgb(124, 162, 177),
                rgb(185, 218, 233),
                rgb(165, 197, 213),
            ),
            AppTheme::Dreams => Self::from_stops(
                rgb(33, 11, 75),
                rgb(63, 28, 109),
                rgb(106, 42, 152),
                rgb(255, 61, 148),
                rgb(181, 48, 126),
            ),
            AppTheme::Paranoid => Self::from_stops(
                rgb(29, 29, 78),
                rgb(63, 63, 136),
                rgb(95, 95, 191),
                rgb(210, 210, 244),
                rgb(154, 154, 224),
            ),
            AppTheme::RedVelvet => Self::from_stops(
                rgb(26, 15, 15),
                rgb(60, 20, 20),
                rgb(139, 35, 35),
                rgb(255, 220, 220),
                rgb(220, 60, 60),
            ),
            AppTheme::Subspace => Self::from_stops(
                rgb(46, 26, 71),
                rgb(74, 42, 106),
                rgb(121, 75, 139),
                rgb(226, 199, 230),
                rgb(183, 123, 180),
            ),
            AppTheme::Tiefling => Self::from_stops(
                rgb(58, 10, 77),
                rgb(113, 29, 154),
                rgb(164, 45, 180),
                rgb(249, 197, 78),
                rgb(255, 92, 138),
            ),
            AppTheme::Vibes => Self::from_stops(
                rgb(15, 15, 30),
                rgb(30, 30, 60),
                rgb(204, 0, 255),
                rgb(0, 255, 204),
                rgb(255, 204, 0),
            ),
        }
    }

    fn light() -> Self {
        Self {
            surface0: rgb(245, 245, 245),
            sidebar: rgb(239, 242, 247),
            surface1: rgb(255, 255, 255),
            surface2: rgb(232, 236, 242),
            text: rgb(26, 26, 26),
            muted: rgba(26, 26, 26, 160),
            separator: rgba(26, 26, 26, 24),
            separator_strong: rgba(26, 26, 26, 46),
            accent: rgb(45, 127, 249),
            accent_hover: rgb(72, 145, 255),
            accent_mute: rgba(45, 127, 249, 34),
            success: rgb(31, 168, 98),
            warning: rgb(224, 131, 25),
            error: rgb(217, 59, 59),
            is_dark: false,
        }
    }

    fn dark() -> Self {
        Self {
            surface0: rgb(0, 0, 0),
            sidebar: rgb(5, 6, 8),
            surface1: rgb(10, 10, 11),
            surface2: rgb(14, 20, 28),
            text: rgb(245, 245, 245),
            muted: rgba(245, 245, 245, 160),
            separator: rgba(245, 245, 245, 24),
            separator_strong: rgba(245, 245, 245, 46),
            accent: rgb(45, 127, 249),
            accent_hover: rgb(72, 145, 255),
            accent_mute: rgba(45, 127, 249, 42),
            success: rgb(31, 168, 98),
            warning: rgb(224, 131, 25),
            error: rgb(217, 59, 59),
            is_dark: true,
        }
    }

    fn from_stops(
        surface0: Color32,
        sidebar: Color32,
        surface2: Color32,
        text: Color32,
        accent: Color32,
    ) -> Self {
        Self {
            surface0,
            sidebar,
            surface1: sidebar,
            surface2,
            text,
            muted: with_alpha(text, 166),
            separator: with_alpha(text, 34),
            separator_strong: with_alpha(text, 62),
            accent,
            accent_hover: accent,
            accent_mute: with_alpha(accent, 46),
            success: rgb(31, 168, 98),
            warning: rgb(224, 131, 25),
            error: rgb(217, 59, 59),
            is_dark: true,
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

pub fn configure_ui(ctx: &Context) {
    apply_app_theme(ctx, AppTheme::System);
}

fn apply_app_theme(ctx: &Context, theme: AppTheme) {
    set_style(ctx, egui::Theme::Light, Palette::light());
    set_style(ctx, egui::Theme::Dark, Palette::dark());

    if theme == AppTheme::System {
        ctx.set_theme(egui::ThemePreference::System);
        return;
    }

    let palette = Palette::for_theme(theme, ctx.system_theme());
    let egui_theme = if palette.is_dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    set_style(ctx, egui_theme, palette);
    ctx.set_theme(egui_theme);
}

fn set_style(ctx: &Context, theme: egui::Theme, palette: Palette) {
    let mut style = (*ctx.style_of(theme)).clone();
    let mut visuals = if palette.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    visuals.panel_fill = palette.surface0;
    visuals.window_fill = palette.surface1;
    visuals.faint_bg_color = palette.surface1;
    visuals.extreme_bg_color = palette.surface0;
    visuals.code_bg_color = palette.surface2;
    visuals.hyperlink_color = palette.accent;
    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.noninteractive.fg_stroke.color = palette.text;
    visuals.widgets.inactive.fg_stroke.color = palette.text;
    visuals.widgets.hovered.fg_stroke.color = palette.text;
    visuals.widgets.active.fg_stroke.color = palette.text;
    visuals.widgets.inactive.weak_bg_fill = palette.surface2;
    visuals.widgets.hovered.weak_bg_fill = palette.accent_mute;
    visuals.widgets.active.weak_bg_fill = palette.accent_mute;
    visuals.widgets.inactive.bg_fill = palette.surface2;
    visuals.widgets.hovered.bg_fill = palette.surface2;
    visuals.widgets.active.bg_fill = palette.surface2;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.separator_strong);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    ctx.set_style_of(theme, style);
}

pub struct MouseJigglerApp {
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
    running: Arc<AtomicBool>,
    capture_target: SharedCaptureTarget,
    tray: Option<TrayState>,
    status: String,
    last_error: Option<String>,
    hidden_to_tray: bool,
    restoring_from_tray: bool,
    quit_requested: bool,
    current_page: Page,
    license_document: LicenseDocument,
    license_filter: String,
    license_wrap: bool,
    credits_filter: String,
    icon_texture: Option<egui::TextureHandle>,
    applied_theme: Option<AppTheme>,
    _plasma_watcher: Option<PlasmaTrayWatcher>,
}

impl MouseJigglerApp {
    pub fn new(
        tx: Sender<AppEvent>,
        rx: Receiver<AppEvent>,
        config: Arc<RwLock<AppConfig>>,
        config_path: PathBuf,
        running: Arc<AtomicBool>,
        capture_target: SharedCaptureTarget,
    ) -> Self {
        Self {
            tx,
            rx,
            config,
            config_path,
            running,
            capture_target,
            tray: None,
            status: "Idle.".to_string(),
            last_error: None,
            hidden_to_tray: false,
            restoring_from_tray: false,
            quit_requested: false,
            current_page: Page::Dashboard,
            license_document: LicenseDocument::App,
            license_filter: String::new(),
            license_wrap: false,
            credits_filter: String::new(),
            icon_texture: None,
            applied_theme: None,
            _plasma_watcher: PlasmaTrayWatcher::install(),
        }
    }

    fn ensure_tray(&mut self, ctx: &Context) {
        if self.tray.is_some() {
            return;
        }

        match TrayState::new(self.tx.clone(), ctx.clone(), Arc::clone(&self.running)) {
            Ok(tray) => {
                self.tray = Some(tray);
            }
            Err(error) => {
                self.last_error = Some(format!("Could not create tray icon: {error}"));
            }
        }
    }

    fn handle_events(&mut self, ctx: &Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                AppEvent::BindingCaptured(target, binding) => {
                    self.set_binding(target, binding);
                }
                #[cfg(not(target_os = "linux"))]
                AppEvent::StartRequested => self.start(),
                #[cfg(not(target_os = "linux"))]
                AppEvent::StopRequested => self.stop(),
                AppEvent::ShowWindow => self.show_window(ctx),
                #[cfg(not(target_os = "linux"))]
                AppEvent::QuitRequested => {
                    self.quit_requested = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                AppEvent::Status(status) => {
                    self.status = status;
                    self.last_error = None;
                }
                AppEvent::Error(error) => {
                    self.status = "Stopped.".to_string();
                    self.last_error = Some(error);
                }
            }
        }
    }

    fn show_window(&mut self, ctx: &Context) {
        restore_plasma_window();
        self.hidden_to_tray = false;
        self.restoring_from_tray = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn hide_to_tray(&mut self, ctx: &Context, status: &str) {
        self.hidden_to_tray = true;
        if !hide_plasma_window_to_tray() {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        self.status = status.to_string();
    }

    fn start(&mut self) {
        self.running.store(true, Ordering::SeqCst);
        self.status = "Running.".to_string();
        self.last_error = None;
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.status = "Stopped.".to_string();
    }

    fn set_binding(&mut self, target: BindTarget, binding: Binding) {
        if let Ok(mut config) = self.config.write() {
            match target {
                BindTarget::Start => {
                    if config.stop_binding.as_ref() == Some(&binding) {
                        config.stop_binding = None;
                    }
                    config.start_binding = Some(binding.clone());
                    self.status = format!("Start bound to {}.", binding.display_label());
                }
                BindTarget::Stop => {
                    if config.start_binding.as_ref() == Some(&binding) {
                        config.start_binding = None;
                    }
                    config.stop_binding = Some(binding.clone());
                    self.status = format!("Stop bound to {}.", binding.display_label());
                }
            }

            if let Err(error) = config::save_config(&self.config_path, &config) {
                self.last_error = Some(format!("Could not save config: {error}"));
            } else {
                self.last_error = None;
            }
        }
    }

    fn set_mode(&mut self, mode: MovementMode) {
        if let Ok(mut config) = self.config.write() {
            config.movement_mode = mode;
            self.status = format!("Movement set to {}.", mode.label());
            if let Err(error) = config::save_config(&self.config_path, &config) {
                self.last_error = Some(format!("Could not save config: {error}"));
            } else {
                self.last_error = None;
            }
        }
    }

    fn set_distance(&mut self, distance: MovementDistance) {
        if let Ok(mut config) = self.config.write() {
            config.distance = distance;
            self.status = format!("Distance set to {}.", distance.label());
            if let Err(error) = config::save_config(&self.config_path, &config) {
                self.last_error = Some(format!("Could not save config: {error}"));
            } else {
                self.last_error = None;
            }
        }
    }

    fn set_theme(&mut self, ctx: &Context, theme: AppTheme) {
        if let Ok(mut config) = self.config.write() {
            config.theme = theme;
            self.status = format!("Theme set to {}.", theme.label());
            if let Err(error) = config::save_config(&self.config_path, &config) {
                self.last_error = Some(format!("Could not save config: {error}"));
            } else {
                self.last_error = None;
            }
        }
        apply_app_theme(ctx, theme);
        self.applied_theme = Some(theme);
    }

    fn begin_capture(&mut self, target: BindTarget) {
        if let Ok(mut capture_target) = self.capture_target.lock() {
            capture_target.request = Some(CaptureRequest { target });
        }
        self.status = match target {
            BindTarget::Start => {
                "Press a key, shortcut, or mouse button for start (Esc cancels).".to_string()
            }
            BindTarget::Stop => {
                "Press a key, shortcut, or mouse button for stop (Esc cancels).".to_string()
            }
        };
        self.last_error = None;
    }

    fn capture_target(&self) -> Option<BindTarget> {
        self.capture_target
            .lock()
            .ok()
            .and_then(|slot| slot.request.map(|request| request.target))
    }

    fn config_snapshot(&self) -> AppConfig {
        self.config
            .read()
            .map(|config| config.clone())
            .unwrap_or_default()
    }

    fn apply_theme_from_config(&mut self, ctx: &Context) {
        let theme = self.config_snapshot().theme;
        if self.applied_theme != Some(theme) {
            apply_app_theme(ctx, theme);
            self.applied_theme = Some(theme);
        }
    }

    fn icon_texture(&mut self, ctx: &Context) -> Option<egui::TextureHandle> {
        if self.icon_texture.is_none() {
            if let Ok(icon) = icons::decode_png(icons::RMJ_128_PNG) {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                );
                self.icon_texture = Some(ctx.load_texture("rmj-icon", image, Default::default()));
            }
        }
        self.icon_texture.clone()
    }
}

impl eframe::App for MouseJigglerApp {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.ensure_tray(ctx);
        self.handle_events(ctx);
        self.apply_theme_from_config(ctx);

        let viewport = ctx.input(|input| input.viewport().clone());
        if viewport.close_requested() && !self.quit_requested {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_to_tray(ctx, "Hidden to tray.");
        } else if self.restoring_from_tray {
            if viewport.minimized != Some(true) {
                self.restoring_from_tray = false;
            }
        } else if viewport.minimized == Some(true) && !self.hidden_to_tray {
            self.hide_to_tray(ctx, "Minimized to tray.");
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let config = self.config_snapshot();
        let palette = Palette::for_theme(config.theme, ui.ctx().system_theme());

        egui::Panel::left("rmj_sidebar")
            .exact_size(SIDEBAR_WIDTH)
            .resizable(false)
            .frame(Frame::default().fill(palette.sidebar))
            .show(ui, |ui| self.render_sidebar(ui, palette));

        CentralPanel::default()
            .frame(Frame::default().fill(palette.surface0))
            .show(ui, |ui| match self.current_page {
                Page::Dashboard => self.render_dashboard(ui, &config, palette),
                Page::Settings => self.render_settings(ui, &config, palette),
                Page::About => self.render_about(ui, palette),
                Page::Licenses => self.render_licenses(ui, palette),
                Page::Credits => self.render_credits(ui, palette),
            });
    }
}

#[cfg(target_os = "linux")]
fn hide_plasma_window_to_tray() -> bool {
    run_kwin_window_script(
        "hide",
        r#"
const JIGGLER_WINDOW_IDS = [
    "com.visorcraft.realistic-mouse-jiggler",
    "realistic-mouse-jiggler",
    "realistic mouse jiggler",
];

function windowString(window, key) {
    return String(window[key] || "").toLowerCase();
}

function isJigglerWindow(window) {
    const values = [
        windowString(window, "desktopFileName"),
        windowString(window, "resourceClass"),
        windowString(window, "resourceName"),
        windowString(window, "caption"),
    ];

    return values.some((value) => JIGGLER_WINDOW_IDS.includes(value));
}

const windows = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (const window of windows) {
    if (isJigglerWindow(window)) {
        window.skipTaskbar = true;
        window.minimized = true;
    }
}
"#,
    )
}

#[cfg(not(target_os = "linux"))]
fn hide_plasma_window_to_tray() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub(crate) fn restore_plasma_window() -> bool {
    run_kwin_window_script(
        "show",
        r#"
const JIGGLER_WINDOW_IDS = [
    "com.visorcraft.realistic-mouse-jiggler",
    "realistic-mouse-jiggler",
    "realistic mouse jiggler",
];

function windowString(window, key) {
    return String(window[key] || "").toLowerCase();
}

function isJigglerWindow(window) {
    const values = [
        windowString(window, "desktopFileName"),
        windowString(window, "resourceClass"),
        windowString(window, "resourceName"),
        windowString(window, "caption"),
    ];

    return values.some((value) => JIGGLER_WINDOW_IDS.includes(value));
}

const windows = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (const window of windows) {
    if (isJigglerWindow(window)) {
        window.skipTaskbar = false;
        window.minimized = false;
        workspace.activeWindow = window;
    }
}
"#,
    )
}

#[cfg(not(target_os = "linux"))]
fn restore_plasma_window() -> bool {
    false
}

struct PlasmaTrayWatcher {
    #[cfg(target_os = "linux")]
    plugin_name: String,
}

impl PlasmaTrayWatcher {
    #[cfg(target_os = "linux")]
    fn install() -> Option<Self> {
        if !is_kde_session() || !command_exists("qdbus6") {
            return None;
        }

        let plugin_name = KWIN_TRAY_WATCHER_PLUGIN.to_string();
        let script_path = std::env::temp_dir().join(format!("{plugin_name}.js"));

        if std::fs::write(&script_path, KWIN_TRAY_WATCHER_SCRIPT).is_err() {
            return None;
        }

        let _ = Command::new("qdbus6")
            .args([
                "org.kde.KWin",
                "/Scripting",
                "org.kde.kwin.Scripting.unloadScript",
            ])
            .arg(&plugin_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let loaded = Command::new("qdbus6")
            .args([
                "org.kde.KWin",
                "/Scripting",
                "org.kde.kwin.Scripting.loadScript",
            ])
            .arg(&script_path)
            .arg(&plugin_name)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !loaded {
            let _ = std::fs::remove_file(script_path);
            return None;
        }

        let _ = Command::new("qdbus6")
            .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_file(script_path);

        Some(Self { plugin_name })
    }

    #[cfg(not(target_os = "linux"))]
    fn install() -> Option<Self> {
        None
    }
}

#[cfg(target_os = "linux")]
impl Drop for PlasmaTrayWatcher {
    fn drop(&mut self) {
        unload_plasma_tray_watcher_named(&self.plugin_name);
    }
}

#[cfg(target_os = "linux")]
const KWIN_TRAY_WATCHER_PLUGIN: &str = "realistic-mouse-jiggler-watch";

#[cfg(target_os = "linux")]
pub(crate) fn unload_plasma_tray_watcher() {
    unload_plasma_tray_watcher_named(KWIN_TRAY_WATCHER_PLUGIN);
}

#[cfg(target_os = "linux")]
fn unload_plasma_tray_watcher_named(plugin_name: &str) {
    let _ = Command::new("qdbus6")
        .args([
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.unloadScript",
        ])
        .arg(plugin_name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(target_os = "linux")]
const KWIN_TRAY_WATCHER_SCRIPT: &str = r#"
const JIGGLER_WINDOW_IDS = [
    "com.visorcraft.realistic-mouse-jiggler",
    "realistic-mouse-jiggler",
    "realistic mouse jiggler",
];

function windowString(window, key) {
    return String(window[key] || "").toLowerCase();
}

function isJigglerWindow(window) {
    const values = [
        windowString(window, "desktopFileName"),
        windowString(window, "resourceClass"),
        windowString(window, "resourceName"),
        windowString(window, "caption"),
    ];

    return values.some((value) => JIGGLER_WINDOW_IDS.includes(value));
}

function syncJigglerTaskbar(window) {
    if (!isJigglerWindow(window)) {
        return;
    }

    if (window.minimized && !window.skipTaskbar) {
        window.skipTaskbar = true;
    } else if (!window.minimized && window.skipTaskbar) {
        window.skipTaskbar = false;
    }
}

function watchJiggler(window) {
    if (!isJigglerWindow(window)) {
        return;
    }

    syncJigglerTaskbar(window);
    if (window.minimizedChanged) {
        window.minimizedChanged.connect(() => syncJigglerTaskbar(window));
    }
}

const windows = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (const window of windows) {
    watchJiggler(window);
}

if (workspace.windowAdded) {
    workspace.windowAdded.connect(watchJiggler);
} else if (workspace.clientAdded) {
    workspace.clientAdded.connect(watchJiggler);
}
"#;

#[cfg(target_os = "linux")]
fn run_kwin_window_script(action: &str, script: &str) -> bool {
    if !is_kde_session() || !command_exists("qdbus6") {
        return false;
    }

    let plugin_name = format!("realistic-mouse-jiggler-{action}-{}", std::process::id());
    let script_path = std::env::temp_dir().join(format!("{plugin_name}.js"));

    if std::fs::write(&script_path, script).is_err() {
        return false;
    }

    let loaded = Command::new("qdbus6")
        .args([
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.loadScript",
        ])
        .arg(&script_path)
        .arg(&plugin_name)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if loaded {
        let _ = Command::new("qdbus6")
            .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("qdbus6")
            .args([
                "org.kde.KWin",
                "/Scripting",
                "org.kde.kwin.Scripting.unloadScript",
            ])
            .arg(&plugin_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = std::fs::remove_file(script_path);
    loaded
}

#[cfg(target_os = "linux")]
fn is_kde_session() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|desktop| desktop.to_ascii_lowercase().contains("kde"))
        .unwrap_or(false)
        || std::env::var("KDE_FULL_SESSION")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

impl MouseJigglerApp {
    fn render_sidebar(&mut self, ui: &mut Ui, palette: Palette) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        ui.set_width(ui.available_width());
        ui.add_space(18.0);

        ui.horizontal(|ui| {
            ui.add_space(14.0);
            if let Some(texture) = self.icon_texture(ui.ctx()) {
                ui.add(egui::Image::from_texture((
                    texture.id(),
                    egui::vec2(44.0, 44.0),
                )));
            }
            ui.vertical(|ui| {
                ui.add_space(3.0);
                ui.label(strong("Realistic Mouse Jiggler", palette, 17.0));
                ui.label(muted("Human-like cursor movement", palette, 12.0));
            });
        });

        ui.add_space(24.0);
        sidebar_section_label(ui, palette, "WORKSPACE");
        self.sidebar_item(ui, palette, Page::Dashboard, "D", "Dashboard");

        ui.add_space(20.0);
        sidebar_section_label(ui, palette, "TOOLS");
        self.sidebar_item(ui, palette, Page::Settings, "⚙", "Settings");
        self.sidebar_item(ui, palette, Page::About, "i", "About");

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let version = format!("v{}", env!("CARGO_PKG_VERSION"));
                let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 22.0), Sense::hover());
                ui.painter().rect(
                    rect,
                    11.0,
                    Color32::TRANSPARENT,
                    Stroke::new(1.0, palette.separator_strong),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    version,
                    FontId::monospace(11.0),
                    palette.muted,
                );
            });
        });
    }

    fn sidebar_item(&mut self, ui: &mut Ui, palette: Palette, page: Page, icon: &str, label: &str) {
        let active = if page == Page::About {
            self.current_page.about_group()
        } else {
            self.current_page == page
        };
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2((ui.available_width() - 8.0).max(1.0), 34.0),
            Sense::click(),
        );
        if response.clicked() {
            self.current_page = page;
        }

        let fill = if active {
            palette.accent_mute
        } else if response.hovered() {
            palette.surface1
        } else {
            Color32::TRANSPARENT
        };
        let row_rect = rect.shrink2(egui::vec2(6.0, 0.0));
        ui.painter().rect_filled(row_rect, 7.0, fill);
        let color = if active { palette.accent } else { palette.text };
        let icon_center = row_rect.left_center() + egui::vec2(18.0, 0.0);
        let icon_bg = if fill == Color32::TRANSPARENT {
            palette.sidebar
        } else {
            fill
        };
        paint_sidebar_icon(ui, page, icon, icon_center, color, icon_bg);
        ui.painter().text(
            row_rect.left_center() + egui::vec2(40.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(14.0),
            color,
        );
    }

    fn render_dashboard(&mut self, ui: &mut Ui, config: &AppConfig, palette: Palette) {
        page_header(
            ui,
            palette,
            "Dashboard",
            "Start, stop, and monitor realistic cursor movement.",
        );
        ScrollArea::vertical().show(ui, |ui| {
            body(ui, |ui| {
                let is_running = self.running.load(Ordering::SeqCst);
                card(ui, palette, |ui| {
                    ui.horizontal(|ui| {
                        status_dot(ui, palette, is_running);
                        ui.vertical(|ui| {
                            ui.label(strong(
                                if is_running { "Running" } else { "Stopped" },
                                palette,
                                28.0,
                            ));
                            ui.add(
                                Label::new(RichText::new(&self.status).color(palette.muted)).wrap(),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            self.action_buttons(ui, palette, is_running);
                        });
                    });
                });

                ui.add_space(16.0);
                card(ui, palette, |ui| {
                    ui.label(strong("Global bindings", palette, 16.0));
                    ui.label(muted(
                        "Bindings work while the window is hidden in the tray.",
                        palette,
                        13.0,
                    ));
                    ui.add_space(10.0);
                    let capture_target = self.capture_target();
                    egui::Grid::new("binding_grid")
                        .num_columns(2)
                        .spacing(egui::vec2(18.0, 12.0))
                        .show(ui, |ui| {
                            form_label(ui, palette, "Start");
                            self.binding_control(
                                ui,
                                palette,
                                config.start_binding.as_ref(),
                                BindTarget::Start,
                                capture_target,
                            );
                            ui.end_row();

                            form_label(ui, palette, "Stop");
                            self.binding_control(
                                ui,
                                palette,
                                config.stop_binding.as_ref(),
                                BindTarget::Stop,
                                capture_target,
                            );
                            ui.end_row();
                        });
                });

                if let Some(error) = &self.last_error {
                    ui.add_space(16.0);
                    error_card(ui, palette, error);
                }
            });
        });
    }

    fn render_settings(&mut self, ui: &mut Ui, config: &AppConfig, palette: Palette) {
        page_header(ui, palette, "Settings", "Autosaved to config.toml.");
        ScrollArea::vertical().show(ui, |ui| {
            body(ui, |ui| {

            card(ui, palette, |ui| {
                ui.label(strong("Movement", palette, 16.0));
                ui.label(muted(
                    "Choose the cursor movement style and how far it travels. Distance applies on the next start.",
                    palette,
                    13.0,
                ));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Style").color(palette.text));
                    ui.add_space(20.0);
                    let mut movement = config.movement_mode;
                    for mode in MovementMode::ALL {
                        if ui.radio_value(&mut movement, mode, mode.label()).clicked() {
                            self.set_mode(mode);
                        }
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Distance").color(palette.text));
                    ui.add_space(20.0);
                    let mut selected = config.distance;
                    let combo_id = ui.make_persistent_id("distance_picker");
                    let response = egui::ComboBox::from_id_salt("distance_picker")
                        .width(260.0)
                        .selected_text(selected.label())
                        .show_ui(ui, |ui| {
                            for distance in MovementDistance::ALL {
                                ui.selectable_value(&mut selected, distance, distance.label());
                            }
                        })
                        .response;
                    if response.clicked() {
                        response.request_focus();
                    }
                    if response.has_focus() || egui::ComboBox::is_open(ui.ctx(), combo_id) {
                        let direction = ui.input(|input| {
                            if input.key_pressed(egui::Key::ArrowDown) {
                                1
                            } else if input.key_pressed(egui::Key::ArrowUp) {
                                -1
                            } else {
                                0
                            }
                        });
                        if direction != 0 {
                            selected = adjacent_distance(selected, direction);
                        }
                    }
                    if selected != config.distance {
                        self.set_distance(selected);
                    }
                });
            });

            ui.add_space(16.0);
            card(ui, palette, |ui| {
                ui.label(strong("Appearance", palette, 16.0));
                ui.label(muted("Theme changes apply immediately and persist across launches.", palette, 13.0));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Theme").color(palette.text));
                    ui.add_space(20.0);
                    let mut selected = config.theme;
                    let combo_id = ui.make_persistent_id("theme_picker");
                    let response = egui::ComboBox::from_id_salt("theme_picker")
                        .width(260.0)
                        .selected_text(selected.label())
                        .show_ui(ui, |ui| {
                            for theme in AppTheme::ALL {
                                ui.selectable_value(&mut selected, theme, theme.label());
                            }
                        })
                        .response;
                    if response.clicked() {
                        response.request_focus();
                    }
                    if response.has_focus() || egui::ComboBox::is_open(ui.ctx(), combo_id) {
                        let direction = ui.input(|input| {
                            if input.key_pressed(egui::Key::ArrowDown) {
                                1
                            } else if input.key_pressed(egui::Key::ArrowUp) {
                                -1
                            } else {
                                0
                            }
                        });
                        if direction != 0 {
                            selected = adjacent_theme(selected, direction);
                        }
                    }
                    if selected != config.theme {
                        self.set_theme(ui.ctx(), selected);
                    }
                });
            });

            ui.add_space(16.0);
            card(ui, palette, |ui| {
                ui.label(strong("Tray behavior", palette, 16.0));
                ui.add(Label::new(RichText::new("Minimize or close the window to keep Realistic Mouse Jiggler reachable from the system tray. Tray Start, Stop, Open, and Quit continue to work while hidden.").color(palette.muted)).wrap());
            });

            if let Some(error) = &self.last_error {
                ui.add_space(16.0);
                error_card(ui, palette, error);
            }
            });
        });
    }

    fn render_about(&mut self, ui: &mut Ui, palette: Palette) {
        page_header(
            ui,
            palette,
            "About",
            "Built on Rust + egui for Linux, macOS, and Windows.",
        );
        ScrollArea::vertical().show(ui, |ui| {
            body(ui, |ui| {
            let texture = self.icon_texture(ui.ctx());
            card(ui, palette, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    if let Some(texture) = texture {
                        ui.add(egui::Image::from_texture((texture.id(), egui::vec2(112.0, 112.0))));
                    }
                    ui.add_space(24.0);
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        ui.label(strong("Realistic Mouse Jiggler", palette, 30.0));
                        ui.add(Label::new(RichText::new("Human-like cursor movement — with tray controls and global bindings.").color(palette.muted).size(15.0)).wrap());
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            pill(ui, palette, &format!("v{}", env!("CARGO_PKG_VERSION")), palette.accent, palette.accent_mute);
                            pill(ui, palette, "MIT", palette.text, palette.surface2);
                            pill(ui, palette, platform_label(), palette.text, palette.surface2);
                        });
                    });
                });
            });

            ui.add_space(22.0);
            section_label(ui, palette, "WHAT'S INSIDE");
            ui.columns(2, |columns| {
                feature_card(&mut columns[0], palette, "Realistic movement", "Smooth, varied cursor paths help avoid robotic motion.", "R");
                feature_card(&mut columns[1], palette, "Tray controls", "Start, stop, open, and quit from the native system tray.", "T");
            });
            ui.add_space(16.0);
            ui.columns(2, |columns| {
                feature_card(&mut columns[0], palette, "Global bindings", "Keyboard and mouse shortcuts keep working while hidden.", "K");
                feature_card(&mut columns[1], palette, "Cross-platform", "Linux, macOS, and Windows builds with native packaging.", "X");
            });

            ui.add_space(22.0);
            link_card(
                ui,
                palette,
                "Cross-platform desktop mouse jiggler built with Rust and egui.",
                "github.com/visorcraft/realistic-mouse-jiggler",
                "Visit project",
                "https://github.com/visorcraft/realistic-mouse-jiggler",
            );

            ui.add_space(22.0);
            card(ui, palette, |ui| {
                ui.label(strong("Licenses & Credits", palette, 16.0));
                ui.label(muted("Every direct + transitive crate, acknowledgments, and full license text is bundled in the built-in licenses view.", palette, 13.0));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if flat_button(ui, palette, "Licenses").clicked() {
                        self.current_page = Page::Licenses;
                    }
                    if flat_button(ui, palette, "Credits").clicked() {
                        self.current_page = Page::Credits;
                    }
                });
            });

            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(muted("Built by VisorCraft · Powered by Rust + egui", palette, 13.0));
            });
            });
        });
    }

    fn render_licenses(&mut self, ui: &mut Ui, palette: Palette) {
        page_header(
            ui,
            palette,
            "Licenses",
            "Bundled license and attribution documents, available without opening a browser.",
        );
        body(ui, |ui| {
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                for document in LicenseDocument::ALL {
                    if ui
                        .selectable_label(self.license_document == document, document.label())
                        .clicked()
                    {
                        self.license_document = document;
                        self.license_filter.clear();
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if flat_button(ui, palette, "Copy").clicked() {
                        ui.ctx().copy_text(self.license_document.body().to_string());
                        self.status = "Copied license document.".to_string();
                    }
                });
            });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(strong(self.license_document.label(), palette, 17.0));
                    ui.add(
                        Label::new(
                            RichText::new(self.license_document.subtitle()).color(palette.muted),
                        )
                        .wrap(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let body = self.license_document.body();
                    let count = if self.license_filter.trim().is_empty() {
                        line_count(body)
                    } else {
                        count_matching_lines(body, &self.license_filter)
                    };
                    ui.label(muted(format!("{count} lines"), palette, 12.0));
                });
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let filter_width = (ui.available_width() - 150.0).max(160.0);
                let filter = TextEdit::singleline(&mut self.license_filter)
                    .hint_text("Find by crate, package, license, or phrase...")
                    .desired_width(filter_width);
                ui.add_sized([filter_width, 24.0], filter);
                ui.checkbox(&mut self.license_wrap, "Wrap");
                if flat_button(ui, palette, "Clear").clicked() {
                    self.license_filter.clear();
                }
            });

            ui.add_space(10.0);
            let visible = filtered_body(self.license_document.body(), &self.license_filter);
            let mut text = visible;
            Frame::default()
                .fill(palette.surface1)
                .stroke(Stroke::new(1.0, palette.separator))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let edit = TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                .desired_rows(28)
                                .desired_width(ui.available_width())
                                .interactive(false);
                            ui.add_sized([ui.available_width(), 520.0], edit);
                        });
                });
        });
    }

    fn render_credits(&mut self, ui: &mut Ui, palette: Palette) {
        let crate_count = third_party_credits().len();
        page_header(
            ui,
            palette,
            "Credits",
            &format!("{crate_count} Cargo crates and platform integrations."),
        );
        ScrollArea::vertical().show(ui, |ui| {
            body(ui, |ui| {
            card(ui, palette, |ui| {
                ui.label(strong("Runtime components", palette, 16.0));
                ui.add(Label::new(RichText::new("System services and optional helpers Realistic Mouse Jiggler integrates with at runtime.").color(palette.muted)).wrap());
                ui.add_space(10.0);
                runtime_row(ui, palette, "egui / eframe", "MIT OR Apache-2.0", "https://github.com/emilk/egui");
                runtime_row(ui, palette, "enigo", "MIT", "https://github.com/enigo-rs/enigo");
                runtime_row(ui, palette, "ksni StatusNotifierItem tray", "MIT", "https://github.com/iovxw/ksni");
                runtime_row(ui, palette, "tray-icon", "MIT OR Apache-2.0", "https://github.com/tauri-apps/tray-icon");
                runtime_row(ui, palette, "ydotool (optional Wayland backend)", "AGPL-3.0-or-later", "https://github.com/ReimuNotMoe/ydotool");
            });

            ui.add_space(18.0);
            section_label(ui, palette, "CARGO CRATES");
            ui.horizontal(|ui| {
                let count_width = 96.0;
                let filter_width = (ui.available_width() - count_width - 12.0).max(160.0);
                ui.add_sized(
                    [filter_width, 24.0],
                    TextEdit::singleline(&mut self.credits_filter)
                        .hint_text("Filter by crate name or license...")
                        .desired_width(filter_width),
                );
                let filtered = filtered_credits_count(&self.credits_filter);
                ui.add_sized(
                    [count_width, 24.0],
                    Label::new(muted(format!("{filtered} / {crate_count}"), palette, 12.0)),
                );
            });

            ui.add_space(10.0);
            Frame::default()
                .fill(palette.surface1)
                .stroke(Stroke::new(1.0, palette.separator))
                .corner_radius(10.0)
                .show(ui, |ui| {
                    credit_header(ui, palette);
                    ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        let query = self.credits_filter.trim().to_ascii_lowercase();
                        for credit in third_party_credits() {
                            if !query.is_empty()
                                && !credit.name.to_ascii_lowercase().contains(&query)
                                && !credit.version.to_ascii_lowercase().contains(&query)
                                && !credit.license.to_ascii_lowercase().contains(&query)
                            {
                                continue;
                            }
                            credit_row(ui, palette, credit);
                        }
                    });
                });
            });
        });
    }

    fn action_buttons(&mut self, ui: &mut Ui, palette: Palette, is_running: bool) {
        let start = ui.add_enabled(
            !is_running,
            Button::new(RichText::new("START").color(Color32::WHITE).strong())
                .fill(if is_running {
                    palette.surface2
                } else {
                    palette.accent
                })
                .stroke(Stroke::new(1.0, palette.accent))
                .min_size(ACTION_BUTTON_SIZE),
        );
        if start.clicked() {
            self.start();
        }

        let stop = ui.add_enabled(
            is_running,
            Button::new(
                RichText::new("STOP")
                    .color(if is_running {
                        Color32::WHITE
                    } else {
                        palette.muted
                    })
                    .strong(),
            )
            .fill(if is_running {
                palette.error
            } else {
                palette.surface2
            })
            .stroke(Stroke::new(1.0, palette.separator_strong))
            .min_size(ACTION_BUTTON_SIZE),
        );
        if stop.clicked() || stop.is_pointer_button_down_on() {
            self.stop();
        }
    }

    fn binding_control(
        &mut self,
        ui: &mut Ui,
        palette: Palette,
        binding: Option<&Binding>,
        target: BindTarget,
        capture_target: Option<BindTarget>,
    ) {
        let is_capturing = capture_target == Some(target);
        let text = if is_capturing {
            "PRESS A KEY, SHORTCUT, OR MOUSE BUTTON".to_string()
        } else {
            binding
                .map(|binding| binding.display_label().to_string())
                .unwrap_or_else(|| "PRESS A KEY, SHORTCUT, OR MOUSE BUTTON".to_string())
        };

        let button = Button::new(RichText::new(text).color(Color32::WHITE))
            .fill(if is_capturing {
                palette.accent_hover
            } else {
                palette.accent
            })
            .stroke(Stroke::new(1.0, palette.accent))
            .min_size(egui::vec2(300.0, 36.0));
        if ui.add(button).clicked() {
            self.begin_capture(target);
        }
    }
}

fn page_header(ui: &mut Ui, palette: Palette, title: &str, subtitle: &str) {
    Frame::default()
        .fill(palette.surface0)
        .inner_margin(egui::Margin::symmetric(24, 16))
        .show(ui, |ui| {
            ui.label(strong(title, palette, 24.0));
            ui.label(muted(subtitle, palette, 13.0));
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, palette.separator),
    );
    ui.add_space(18.0);
}

fn body<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    Frame::default()
        .inner_margin(egui::Margin {
            left: 24,
            right: 24,
            top: 0,
            bottom: 24,
        })
        .show(ui, |ui| {
            content_width(ui);
            add_contents(ui)
        })
}

fn content_width(ui: &mut Ui) {
    ui.set_width(ui.available_width());
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
}

fn card<R>(
    ui: &mut Ui,
    palette: Palette,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    Frame::default()
        .fill(palette.surface1)
        .stroke(Stroke::new(1.0, palette.separator))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui)
        })
}

fn feature_card(ui: &mut Ui, palette: Palette, title: &str, body: &str, icon: &str) {
    card(ui, palette, |ui| {
        ui.horizontal(|ui| {
            icon_tile(ui, palette, icon, 44.0);
            ui.vertical(|ui| {
                ui.label(strong(title, palette, 15.0));
                ui.add(Label::new(RichText::new(body).color(palette.muted).size(13.0)).wrap());
            });
        });
    });
}

fn link_card(ui: &mut Ui, palette: Palette, title: &str, url_label: &str, button: &str, url: &str) {
    card(ui, palette, |ui| {
        ui.horizontal(|ui| {
            icon_tile(ui, palette, "↗", 56.0);
            ui.vertical(|ui| {
                ui.label(strong(title, palette, 15.0));
                ui.hyperlink_to(url_label, url);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if flat_button(ui, palette, button).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            });
        });
    });
}

fn icon_tile(ui: &mut Ui, palette: Palette, icon: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    ui.painter().rect(
        rect,
        10.0,
        palette.accent_mute,
        Stroke::new(1.0, with_alpha(palette.accent, 90)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(size * 0.45),
        palette.accent,
    );
}

fn status_dot(ui: &mut Ui, palette: Palette, running: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(54.0, 54.0), Sense::hover());
    let color = if running {
        palette.success
    } else {
        palette.muted
    };
    ui.painter().circle_filled(rect.center(), 12.0, color);
    ui.painter()
        .circle_stroke(rect.center(), 24.0, Stroke::new(1.0, with_alpha(color, 90)));
}

fn pill(ui: &mut Ui, palette: Palette, text: &str, color: Color32, fill: Color32) {
    Frame::default()
        .fill(fill)
        .stroke(Stroke::new(1.0, palette.separator_strong))
        .corner_radius(999.0)
        .inner_margin(egui::Margin::symmetric(12, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).monospace().size(12.0));
        });
}

fn flat_button(ui: &mut Ui, palette: Palette, label: &str) -> egui::Response {
    ui.add(
        Button::new(RichText::new(label).color(palette.text))
            .fill(palette.surface2)
            .stroke(Stroke::new(1.0, palette.separator_strong)),
    )
}

fn section_label(ui: &mut Ui, palette: Palette, label: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(label)
            .color(palette.muted)
            .size(10.0)
            .strong(),
    );
    ui.add_space(2.0);
}

fn sidebar_section_label(ui: &mut Ui, palette: Palette, label: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(
            RichText::new(label)
                .color(palette.muted)
                .size(10.0)
                .strong(),
        );
    });
    ui.add_space(2.0);
}

fn paint_sidebar_icon(
    ui: &Ui,
    page: Page,
    fallback: &str,
    center: egui::Pos2,
    color: Color32,
    background: Color32,
) {
    let painter = ui.painter();
    match page {
        // Matches Grexa's Kirigami/Breeze `go-home-symbolic` silhouette.
        Page::Dashboard => {
            let scale = 18.0 / 22.0;
            let origin = center - egui::vec2(11.0 * scale, 11.0 * scale);
            let pt = |x: f32, y: f32| origin + egui::vec2(x * scale, y * scale);
            painter.add(egui::epaint::PathShape::convex_polygon(
                vec![pt(3.0, 11.0), pt(11.0, 3.0), pt(19.0, 11.0)],
                color,
                Stroke::NONE,
            ));
            painter.rect_filled(
                egui::Rect::from_two_pos(pt(4.0, 10.5), pt(18.0, 19.0)),
                0.0,
                color,
            );
            painter.rect_filled(
                egui::Rect::from_two_pos(pt(13.0, 5.0), pt(16.0, 9.0)),
                0.0,
                color,
            );
            painter.rect_filled(
                egui::Rect::from_two_pos(pt(10.0, 14.0), pt(13.0, 19.0)),
                0.0,
                background,
            );
        }
        // Matches Grexa's Kirigami/Breeze `help-about-symbolic` circle-i.
        Page::About => {
            painter.circle_stroke(center, 7.0, Stroke::new(1.2, color));
            painter.rect_filled(
                egui::Rect::from_center_size(center + egui::vec2(0.0, 2.5), egui::vec2(2.0, 7.0)),
                0.0,
                color,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(center + egui::vec2(0.0, -4.0), egui::vec2(2.0, 2.0)),
                0.0,
                color,
            );
        }
        _ => {
            painter.text(
                center,
                Align2::CENTER_CENTER,
                fallback,
                FontId::proportional(14.0),
                color,
            );
        }
    }
}

fn form_label(ui: &mut Ui, palette: Palette, label: &str) {
    ui.label(RichText::new(label).strong().color(palette.text));
}

fn strong(text: impl Into<String>, palette: Palette, size: f32) -> RichText {
    RichText::new(text.into())
        .color(palette.text)
        .strong()
        .size(size)
}

fn muted(text: impl Into<String>, palette: Palette, size: f32) -> RichText {
    RichText::new(text.into()).color(palette.muted).size(size)
}

fn error_card(ui: &mut Ui, palette: Palette, error: &str) {
    Frame::default()
        .fill(with_alpha(palette.warning, 28))
        .stroke(Stroke::new(1.0, palette.warning))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.add(Label::new(RichText::new(error).color(palette.text)).wrap());
        });
}

fn platform_label() -> &'static str {
    if cfg!(target_os = "linux") {
        "Linux · egui"
    } else if cfg!(target_os = "macos") {
        "macOS · egui"
    } else if cfg!(target_os = "windows") {
        "Windows · egui"
    } else {
        "egui"
    }
}

fn adjacent_theme(current: AppTheme, direction: isize) -> AppTheme {
    let all = AppTheme::ALL;
    let len = all.len() as isize;
    let index = all
        .iter()
        .position(|theme| *theme == current)
        .unwrap_or_default() as isize;
    all[(index + direction).rem_euclid(len) as usize]
}

fn adjacent_distance(current: MovementDistance, direction: isize) -> MovementDistance {
    let all = MovementDistance::ALL;
    let len = all.len() as isize;
    let index = all
        .iter()
        .position(|distance| *distance == current)
        .unwrap_or_default() as isize;
    all[(index + direction).rem_euclid(len) as usize]
}

fn line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

fn count_matching_lines(text: &str, query: &str) -> usize {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return 0;
    }
    text.lines()
        .filter(|line| line.to_ascii_lowercase().contains(&needle))
        .count()
}

fn filtered_body(text: &str, query: &str) -> String {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return text.to_owned();
    }

    let matches: Vec<String> = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if line.to_ascii_lowercase().contains(&needle) {
                Some(format!("{:>5}  {line}", index + 1))
            } else {
                None
            }
        })
        .collect();

    if matches.is_empty() {
        format!("No matches for '{query}'.")
    } else {
        matches.join("\n")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThirdPartyCredit {
    name: String,
    version: String,
    license: String,
    url: String,
}

fn third_party_credits() -> &'static [ThirdPartyCredit] {
    static CREDITS: OnceLock<Vec<ThirdPartyCredit>> = OnceLock::new();
    CREDITS.get_or_init(|| third_party_credit_entries(THIRD_PARTY_LICENSES_TEXT))
}

fn third_party_credit_entries(text: &str) -> Vec<ThirdPartyCredit> {
    let mut current_license = String::new();
    let mut in_license_texts = false;
    let mut in_used_by = false;
    let mut entries = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed == "## License Texts" {
            in_license_texts = true;
            continue;
        }
        if !in_license_texts {
            continue;
        }

        if let Some(title) = trimmed.strip_prefix("### ") {
            current_license = license_section_to_spdx(title);
            in_used_by = false;
            continue;
        }

        if trimmed == "Used by:" {
            in_used_by = !current_license.is_empty();
            continue;
        }

        if !in_used_by {
            continue;
        }

        if trimmed.starts_with("```") || trimmed == "---" {
            in_used_by = false;
            continue;
        }

        if let Some(entry) = parse_used_by_line(trimmed, &current_license) {
            entries.push(entry);
        }
    }

    entries.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.version.cmp(&b.version))
            .then_with(|| a.license.cmp(&b.license))
    });
    entries
}

fn parse_used_by_line(line: &str, license: &str) -> Option<ThirdPartyCredit> {
    let body = line.strip_prefix("- [`")?;
    let (label, rest) = body.split_once("`](")?;
    let url = rest.strip_suffix(')')?;
    let (name, version) = label.rsplit_once(' ')?;

    Some(ThirdPartyCredit {
        name: name.to_owned(),
        version: version.to_owned(),
        license: license.to_owned(),
        url: url.to_owned(),
    })
}

fn license_section_to_spdx(title: &str) -> String {
    match title {
        "Apache License 2.0" => "Apache-2.0".to_owned(),
        "BSD 2-Clause &quot;Simplified&quot; License" => "BSD-2-Clause".to_owned(),
        "BSD 3-Clause &quot;New&quot; or &quot;Revised&quot; License" => "BSD-3-Clause".to_owned(),
        "BSD Zero Clause License" => "0BSD".to_owned(),
        "Boost Software License 1.0" => "BSL-1.0".to_owned(),
        "Creative Commons Zero v1.0 Universal" => "CC0-1.0".to_owned(),
        "ISC License" => "ISC".to_owned(),
        "MIT License" => "MIT".to_owned(),
        "Mozilla Public License 2.0" => "MPL-2.0".to_owned(),
        "SIL Open Font License 1.1" => "OFL-1.1".to_owned(),
        "The Unlicense" => "Unlicense".to_owned(),
        "Ubuntu Font Licence v1.0" => "Ubuntu-font-1.0".to_owned(),
        "Unicode License v3" => "Unicode-3.0".to_owned(),
        "zlib License" => "Zlib".to_owned(),
        other => other
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">"),
    }
}

fn filtered_credits_count(query: &str) -> usize {
    let needle = query.trim().to_ascii_lowercase();
    third_party_credits()
        .iter()
        .filter(|credit| {
            needle.is_empty()
                || credit.name.to_ascii_lowercase().contains(&needle)
                || credit.version.to_ascii_lowercase().contains(&needle)
                || credit.license.to_ascii_lowercase().contains(&needle)
        })
        .count()
}

fn runtime_row(ui: &mut Ui, palette: Palette, name: &str, license: &str, url: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).color(palette.text).strong());
        ui.label(RichText::new(license).color(palette.muted).monospace());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.hyperlink_to("Open", url);
        });
    });
}

fn credit_columns(width: f32) -> (egui::Rangef, egui::Rangef, egui::Rangef, egui::Rangef) {
    let gutter = 14.0;
    let project = 58.0;
    let version = 82.0;
    let name = (width * 0.34).clamp(170.0, 260.0);
    let license = (width - name - version - project - gutter * 3.0).max(140.0);

    let name_range = egui::Rangef::new(0.0, name);
    let version_range =
        egui::Rangef::new(name_range.max + gutter, name_range.max + gutter + version);
    let license_range = egui::Rangef::new(
        version_range.max + gutter,
        version_range.max + gutter + license,
    );
    let project_range = egui::Rangef::new(width - project, width);
    (name_range, version_range, license_range, project_range)
}

fn credit_cell_rect(row: egui::Rect, range: egui::Rangef) -> egui::Rect {
    egui::Rect::from_min_max(
        row.left_top() + egui::vec2(range.min, 0.0),
        row.left_top() + egui::vec2(range.max, row.height()),
    )
}

fn credit_header(ui: &mut Ui, palette: Palette) {
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::hover());
    ui.painter().rect_filled(row, 0.0, palette.surface2);
    let (name, version, license, project) = credit_columns(width);
    credit_header_text(ui, row, name, "Crate", palette);
    credit_header_text(ui, row, version, "Version", palette);
    credit_header_text(ui, row, license, "License expression", palette);
    credit_header_text(ui, row, project, "Project", palette);
}

fn credit_header_text(ui: &Ui, row: egui::Rect, range: egui::Rangef, text: &str, palette: Palette) {
    ui.painter().text(
        credit_cell_rect(row, range).center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        palette.text,
    );
}

fn credit_row(ui: &mut Ui, palette: Palette, credit: &ThirdPartyCredit) {
    let width = ui.available_width().min(ui.clip_rect().width());
    let (row, _) = ui.allocate_exact_size(egui::vec2(width, 38.0), Sense::hover());
    let (name, version, license, project) = credit_columns(width);

    ui.put(
        credit_cell_rect(row, name),
        Label::new(RichText::new(&credit.name).color(palette.text).monospace()).truncate(),
    );
    ui.put(
        credit_cell_rect(row, version),
        Label::new(
            RichText::new(&credit.version)
                .color(palette.muted)
                .monospace(),
        )
        .truncate(),
    );

    let license_rect = credit_cell_rect(row, license).shrink2(egui::vec2(0.0, 6.0));
    ui.painter()
        .rect_filled(license_rect, 999.0, with_alpha(palette.success, 32));
    ui.put(
        license_rect.shrink2(egui::vec2(8.0, 0.0)),
        Label::new(
            RichText::new(&credit.license)
                .color(palette.text)
                .monospace()
                .size(12.0),
        )
        .truncate(),
    );

    ui.put(
        credit_cell_rect(row, project),
        egui::Hyperlink::from_label_and_url("Open", &credit.url),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_license_body_by_matching_line() {
        let filtered = filtered_body("alpha\nbeta license\ngamma", "license");
        assert_eq!(filtered, "    2  beta license");
    }

    #[test]
    fn parses_cargo_about_used_by_line() {
        let entry = parse_used_by_line(
            "- [`egui 0.35.0`](https://github.com/emilk/egui)",
            "MIT OR Apache-2.0",
        )
        .unwrap();
        assert_eq!(entry.name, "egui");
        assert_eq!(entry.version, "0.35.0");
        assert_eq!(entry.license, "MIT OR Apache-2.0");
        assert_eq!(entry.url, "https://github.com/emilk/egui");
    }
}
