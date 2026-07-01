use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc, RwLock,
    },
};

use eframe::egui::{
    self, Align, Button, CentralPanel, Color32, Context, Layout, RichText, Stroke, Ui,
    ViewportCommand,
};

use crate::{
    config::{self, AppConfig, Binding, MovementMode},
    events::AppEvent,
    input::{BindTarget, SharedCaptureTarget},
    tray::TrayState,
};

const PANEL_BG: Color32 = Color32::from_rgb(246, 248, 250);
const TEXT: Color32 = Color32::from_rgb(29, 35, 42);
const BUTTON_BG: Color32 = Color32::from_rgb(55, 60, 66);
const BUTTON_BG_HOVER: Color32 = Color32::from_rgb(68, 74, 82);
const BUTTON_TEXT: Color32 = Color32::from_rgb(248, 250, 252);
const DISABLED_BG: Color32 = Color32::from_rgb(210, 215, 221);
const DISABLED_TEXT: Color32 = Color32::from_rgb(118, 126, 136);
const ACTION_BUTTON_SIZE: egui::Vec2 = egui::vec2(96.0, 36.0);

pub fn configure_ui(ctx: &Context) {
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    let mut visuals = egui::Visuals::light();

    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = PANEL_BG;
    visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    visuals.widgets.inactive.fg_stroke.color = TEXT;
    visuals.widgets.hovered.fg_stroke.color = TEXT;
    visuals.widgets.active.fg_stroke.color = TEXT;
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(232, 236, 240);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(221, 226, 232);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(210, 216, 223);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(170, 178, 188));
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(128, 140, 152));
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(94, 106, 120));

    style.visuals = visuals;
    ctx.set_style_of(egui::Theme::Light, style);
    ctx.set_theme(egui::Theme::Light);
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
        }
    }

    fn ensure_tray(&mut self, ctx: &Context) {
        if self.tray.is_some() {
            return;
        }

        match TrayState::new(self.tx.clone(), ctx.clone()) {
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
                AppEvent::StartRequested => self.start(),
                AppEvent::StopRequested => self.stop(),
                AppEvent::ShowWindow => self.show_window(ctx),
                AppEvent::QuitRequested => {
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
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
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
            if let Err(error) = config::save_config(&self.config_path, &config) {
                self.last_error = Some(format!("Could not save config: {error}"));
            }
        }
    }

    fn begin_capture(&mut self, target: BindTarget) {
        if let Ok(mut capture_target) = self.capture_target.lock() {
            *capture_target = Some(target);
        }
        self.status = match target {
            BindTarget::Start => "Press a key or mouse button for start.".to_string(),
            BindTarget::Stop => "Press a key or mouse button for stop.".to_string(),
        };
        self.last_error = None;
    }

    fn capture_target(&self) -> Option<BindTarget> {
        self.capture_target.lock().ok().and_then(|target| *target)
    }

    fn config_snapshot(&self) -> AppConfig {
        self.config
            .read()
            .map(|config| config.clone())
            .unwrap_or_default()
    }
}

impl eframe::App for MouseJigglerApp {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.ensure_tray(ctx);
        self.handle_events(ctx);

        let viewport = ctx.input(|input| input.viewport().clone());
        if viewport.minimized == Some(true) {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            self.status = "Minimized to tray.".to_string();
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        CentralPanel::default()
            .frame(egui::Frame::default().fill(PANEL_BG))
            .show(ui, |ui| self.render_ui(ui));
    }
}

impl MouseJigglerApp {
    fn render_ui(&mut self, ui: &mut Ui) {
        let config = self.config_snapshot();
        let is_running = self.running.load(Ordering::SeqCst);
        let capture_target = self.capture_target();

        ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(RichText::new("Realistic Mouse Jiggler").color(TEXT));
                ui.label(
                    RichText::new(if is_running { "Running" } else { "Stopped" }).color(
                        if is_running {
                            Color32::from_rgb(25, 135, 84)
                        } else {
                            Color32::from_rgb(90, 98, 108)
                        },
                    ),
                )
                .on_hover_text(&self.status);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let color = if is_running {
                    Color32::from_rgb(25, 135, 84)
                } else {
                    Color32::from_rgb(120, 130, 140)
                };
                ui.painter().circle_filled(
                    ui.max_rect().right_top() + egui::vec2(-18.0, 18.0),
                    7.0,
                    color,
                );
            });
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing(egui::vec2(18.0, 14.0))
            .show(ui, |ui| {
                Self::form_label(ui, "Mouse Movement:");
                ui.horizontal(|ui| {
                    let mut selected_mode = config.movement_mode;
                    if ui
                        .radio_value(&mut selected_mode, MovementMode::Realistic, "Realistic")
                        .clicked()
                    {
                        self.set_mode(MovementMode::Realistic);
                    }
                    if ui
                        .radio_value(&mut selected_mode, MovementMode::Simple, "Simple")
                        .clicked()
                    {
                        self.set_mode(MovementMode::Simple);
                    }
                });
                ui.end_row();

                Self::form_label(ui, "Bind start to:");
                self.binding_control(
                    ui,
                    config.start_binding.as_ref(),
                    BindTarget::Start,
                    capture_target,
                );
                ui.end_row();

                Self::form_label(ui, "Bind stop to:");
                self.binding_control(
                    ui,
                    config.stop_binding.as_ref(),
                    BindTarget::Stop,
                    capture_target,
                );
                ui.end_row();
            });

        ui.add_space(12.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ACTION_BUTTON_SIZE.y),
            Layout::left_to_right(Align::Center),
            |ui| {
                let total_width = ACTION_BUTTON_SIZE.x * 2.0 + ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - total_width) / 2.0).max(0.0));

                let start = ui.add_enabled(
                    !is_running,
                    Button::new(RichText::new("START").color(if is_running {
                        DISABLED_TEXT
                    } else {
                        BUTTON_TEXT
                    }))
                    .fill(if is_running { DISABLED_BG } else { BUTTON_BG })
                    .min_size(ACTION_BUTTON_SIZE),
                );
                if start.clicked() {
                    self.start();
                }

                let stop = ui.add_enabled(
                    is_running,
                    Button::new(RichText::new("STOP").color(if is_running {
                        BUTTON_TEXT
                    } else {
                        DISABLED_TEXT
                    }))
                    .fill(if is_running { BUTTON_BG } else { DISABLED_BG })
                    .min_size(ACTION_BUTTON_SIZE),
                );
                if stop.clicked() {
                    self.stop();
                }
            },
        );

        if let Some(error) = &self.last_error {
            ui.add_space(12.0);
            let frame = egui::Frame::default()
                .fill(Color32::from_rgb(255, 244, 230))
                .stroke(Stroke::new(1.0, Color32::from_rgb(230, 175, 90)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 8));
            frame.show(ui, |ui| {
                ui.label(RichText::new(error).color(Color32::from_rgb(112, 70, 20)));
            });
        }
    }

    fn form_label(ui: &mut Ui, label: &str) {
        ui.set_min_width(120.0);
        ui.label(RichText::new(label).strong().color(TEXT));
    }

    fn binding_control(
        &mut self,
        ui: &mut Ui,
        binding: Option<&Binding>,
        target: BindTarget,
        capture_target: Option<BindTarget>,
    ) {
        ui.set_min_width(270.0);
        let is_capturing = capture_target == Some(target);
        let text = if is_capturing {
            "PRESS ANY KEY OR MOUSE BUTTON".to_string()
        } else {
            binding
                .map(|binding| binding.display_label().to_string())
                .unwrap_or_else(|| "PRESS ANY KEY OR MOUSE BUTTON".to_string())
        };

        let button = Button::new(RichText::new(text).color(BUTTON_TEXT))
            .fill(if is_capturing {
                BUTTON_BG_HOVER
            } else {
                BUTTON_BG
            })
            .min_size(egui::vec2(260.0, 34.0));
        if ui.add(button).clicked() {
            self.begin_capture(target);
        }
    }
}
