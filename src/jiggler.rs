use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, RwLock,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
use std::{env, process::Command};

use enigo::{Coordinate, Enigo, Mouse, Settings};

use crate::{config::AppConfig, config::MovementMode, events::AppEvent};

const DISTANCE_X: f64 = 240.0;
const REALISTIC_DISTANCE_Y: f64 = 28.0;
const PERIOD_SECONDS: f64 = 4.0;
const STEP: Duration = Duration::from_millis(30);
const IDLE_STEP: Duration = Duration::from_millis(120);

pub fn spawn_jiggler(
    tx: Sender<AppEvent>,
    running: Arc<AtomicBool>,
    config: Arc<RwLock<AppConfig>>,
) {
    thread::spawn(move || {
        let mut mover: Option<CursorMover> = None;
        let mut current_x = 0;
        let mut current_y = 0;
        let mut started_at = Instant::now();
        let mut was_running = false;

        loop {
            let is_running = running.load(Ordering::SeqCst);

            if !is_running {
                if was_running {
                    if let Some(cursor) = mover.as_mut() {
                        let _ = cursor.move_relative(-current_x, -current_y);
                    }
                    current_x = 0;
                    current_y = 0;
                    was_running = false;
                }
                thread::sleep(IDLE_STEP);
                continue;
            }

            if !was_running {
                started_at = Instant::now();
                current_x = 0;
                current_y = 0;
                was_running = true;
            }

            if mover.is_none() {
                match CursorMover::new() {
                    Ok(cursor) => {
                        let _ = tx.send(AppEvent::Status(format!(
                            "Using {} cursor backend.",
                            cursor.name()
                        )));
                        mover = Some(cursor);
                    }
                    Err(error) => {
                        running.store(false, Ordering::SeqCst);
                        let _ = tx.send(AppEvent::Error(error));
                        continue;
                    }
                }
            }

            let mode = config
                .read()
                .map(|config| config.movement_mode)
                .unwrap_or_default();
            let elapsed = started_at.elapsed().as_secs_f64();
            let (target_x, target_y) = target_position(mode, elapsed);
            let delta_x = target_x - current_x;
            let delta_y = target_y - current_y;

            if delta_x != 0 || delta_y != 0 {
                let move_result = mover
                    .as_mut()
                    .expect("cursor mover should exist")
                    .move_relative(delta_x, delta_y);
                match move_result {
                    Ok(()) => {
                        current_x = target_x;
                        current_y = target_y;
                    }
                    Err(error) => {
                        running.store(false, Ordering::SeqCst);
                        mover = None;
                        let _ = tx.send(AppEvent::Error(error));
                    }
                }
            }

            thread::sleep(STEP);
        }
    });
}

fn target_position(mode: MovementMode, elapsed: f64) -> (i32, i32) {
    let phase = elapsed / PERIOD_SECONDS * std::f64::consts::TAU;

    match mode {
        MovementMode::Simple => ((phase.sin() * DISTANCE_X).round() as i32, 0),
        MovementMode::Realistic => {
            let x = phase.sin() * DISTANCE_X
                + (phase * 2.17 + 0.7).sin() * 22.0
                + (phase * 4.61).sin() * 8.0;
            let y = (phase * 1.31 + 1.2).sin() * REALISTIC_DISTANCE_Y
                + (phase * 3.73 + 0.4).sin() * 7.0;
            (x.round() as i32, y.round() as i32)
        }
    }
}

enum CursorMover {
    Enigo(Box<Enigo>),
    #[cfg(target_os = "linux")]
    Ydotool,
}

impl CursorMover {
    fn new() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            if is_wayland_session() && command_exists("ydotool") {
                ensure_ydotool_ready()?;
                return Ok(Self::Ydotool);
            }
        }

        Enigo::new(&Settings::default())
            .map(Box::new)
            .map(Self::Enigo)
            .map_err(|err| format!("Could not initialize cursor control: {err}"))
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Enigo(_) => "Enigo",
            #[cfg(target_os = "linux")]
            Self::Ydotool => "ydotool",
        }
    }

    fn move_relative(&mut self, delta_x: i32, delta_y: i32) -> Result<(), String> {
        match self {
            Self::Enigo(enigo) => enigo
                .move_mouse(delta_x, delta_y, Coordinate::Rel)
                .map_err(|err| format!("Could not move cursor: {err}")),
            #[cfg(target_os = "linux")]
            Self::Ydotool => ydotool_move(delta_x, delta_y),
        }
    }
}

#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
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

#[cfg(target_os = "linux")]
fn ensure_ydotool_ready() -> Result<(), String> {
    if ydotool_move(0, 0).is_ok() {
        return Ok(());
    }

    let _ = Command::new("systemctl")
        .args(["--user", "start", "ydotool.service"])
        .status();

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if ydotool_move(0, 0).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err("Could not connect to ydotoold. On Wayland, install ydotool and start ydotool.service, or use a desktop portal/libei-compatible setup.".to_string())
}

#[cfg(target_os = "linux")]
fn ydotool_move(delta_x: i32, delta_y: i32) -> Result<(), String> {
    let output = Command::new("ydotool")
        .args([
            "mousemove",
            "--",
            &delta_x.to_string(),
            &delta_y.to_string(),
        ])
        .output()
        .map_err(|err| format!("Could not run ydotool: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "ydotool failed to move the cursor.".to_string()
        } else {
            stderr
        })
    }
}
