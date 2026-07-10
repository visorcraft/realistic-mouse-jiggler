use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, RwLock,
    },
    thread,
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::{env, process::Command, time::Instant};

use enigo::{Coordinate, Enigo, Mouse, Settings};

use crate::{
    config::AppConfig,
    config::MovementDistance,
    config::MovementMode,
    events::AppEvent,
    screens::{self, Bounds},
};

// Default (relative oscillation) behaviour — unchanged.
const DISTANCE_X: f64 = 240.0;
const REALISTIC_DISTANCE_Y: f64 = 28.0;
const PERIOD_SECONDS: f64 = 4.0;

const STEP: Duration = Duration::from_millis(30);
const STEP_SEC: f64 = 0.03;
const IDLE_STEP: Duration = Duration::from_millis(120);

// Edge-to-edge: traverse the full virtual width at a fixed speed so wider
// desktops take longer (no teleporting across a multi-monitor span).
const EDGE_SPEED_PX_PER_SEC: f64 = 1200.0;
const EDGE_WOBBLE_FREQ: f64 = 1.7;
const EDGE_REALISTIC_WOBBLE_Y: f64 = 18.0;

// Random: bounded, re-rolled horizontal bursts. "Bounded commanded
// displacement" — OS edge clipping and the user moving the mouse mean the
// physical offset can differ from what we command, so we clamp what we command
// rather than trusting the physical position.
const RANDOM_SPEED_PX_PER_SEC: f64 = 320.0;
const RANDOM_RADIUS_X: i32 = 640;
const RANDOM_MIN_DURATION_SEC: f64 = 1.0;
const RANDOM_MAX_DURATION_SEC: f64 = 10.0;
const RANDOM_WOBBLE_FREQ: f64 = 2.3;
const RANDOM_REALISTIC_WOBBLE_Y: f64 = 10.0;

// Realistic-mode micro-breaks: while moving, pause every 1..20s for 5..15s,
// then always resume. The motion clock freezes during a pause so the path
// continues from exactly where it stopped.
const PAUSE_MOVE_MIN_SEC: f64 = 1.0;
const PAUSE_MOVE_MAX_SEC: f64 = 20.0;
const PAUSE_MIN_SEC: f64 = 5.0;
const PAUSE_MAX_SEC: f64 = 15.0;

#[derive(Clone, Copy)]
struct RandomState {
    dir: i32,
    duration: f64,
    burst_elapsed: f64,
}

impl RandomState {
    fn new() -> Self {
        Self {
            dir: 1,
            duration: RANDOM_MIN_DURATION_SEC,
            burst_elapsed: 0.0,
        }
    }

    fn reroll(&mut self) {
        self.dir = if fastrand::bool() { 1 } else { -1 };
        self.duration = RANDOM_MIN_DURATION_SEC
            + fastrand::f64() * (RANDOM_MAX_DURATION_SEC - RANDOM_MIN_DURATION_SEC);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PausePhase {
    Moving,
    Pausing,
}

/// Realistic-mode micro-break scheduler. Alternates between a moving phase
/// (1..20s) and a pausing phase (5..15s); a pause always flips back to moving
/// so movement always resumes automatically.
#[derive(Clone, Copy, Debug)]
struct PauseState {
    phase: PausePhase,
    elapsed: f64,
    duration: f64,
}

impl PauseState {
    fn new() -> Self {
        Self {
            phase: PausePhase::Moving,
            elapsed: 0.0,
            duration: PAUSE_MOVE_MIN_SEC,
        }
    }

    fn restart(&mut self) {
        self.phase = PausePhase::Moving;
        self.elapsed = 0.0;
        self.duration = random_moving_duration();
    }

    /// Advance the wall clock by `dt`, flipping phase when an interval ends.
    /// Returns `true` while in a moving phase (caller should move the cursor)
    /// and `false` while pausing (caller holds the cursor still).
    fn update(&mut self, dt: f64) -> bool {
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            self.elapsed = 0.0;
            match self.phase {
                PausePhase::Moving => {
                    self.phase = PausePhase::Pausing;
                    self.duration = random_pause_duration();
                }
                PausePhase::Pausing => {
                    self.phase = PausePhase::Moving;
                    self.duration = random_moving_duration();
                }
            }
        }
        self.phase == PausePhase::Moving
    }
}

fn random_moving_duration() -> f64 {
    PAUSE_MOVE_MIN_SEC + fastrand::f64() * (PAUSE_MOVE_MAX_SEC - PAUSE_MOVE_MIN_SEC)
}

fn random_pause_duration() -> f64 {
    PAUSE_MIN_SEC + fastrand::f64() * (PAUSE_MAX_SEC - PAUSE_MIN_SEC)
}

pub fn spawn_jiggler(
    tx: Sender<AppEvent>,
    running: Arc<AtomicBool>,
    config: Arc<RwLock<AppConfig>>,
) {
    thread::spawn(move || {
        let mut mover: Option<CursorMover> = None;
        let mut was_running = false;

        // Settings captured when a run starts. Distance (and mode) are frozen
        // for the whole run; changes take effect on the next start so the
        // state machine and cursor restoration stay consistent.
        let mut active_mode = MovementMode::Realistic;
        let mut active_distance = MovementDistance::Default;
        // Motion clock: advances only while the cursor is moving, so Realistic
        // pauses freeze the path and resume from the same spot.
        let mut active_elapsed = 0.0_f64;
        let mut pause = PauseState::new();

        // Relative displacement from the origin (Default / Random).
        let mut current_x = 0_i32;
        let mut current_y = 0_i32;

        // Edge-to-edge (absolute) state.
        let mut edge_origin: Option<(i32, i32)> = None;
        let mut edge_bounds: Option<Bounds> = None;
        let mut edge_note_sent = false;

        // Random burst state.
        let mut random = RandomState::new();

        loop {
            let is_running = running.load(Ordering::SeqCst);

            if !is_running {
                if was_running {
                    if let Some(cursor) = mover.as_mut() {
                        restore_cursor(cursor, active_distance, edge_origin, current_x, current_y);
                    }
                    current_x = 0;
                    current_y = 0;
                    edge_origin = None;
                    edge_bounds = None;
                    edge_note_sent = false;
                    was_running = false;
                }
                thread::sleep(IDLE_STEP);
                continue;
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

            if !was_running {
                let snapshot = config
                    .read()
                    .map(|config| (config.movement_mode, config.distance))
                    .unwrap_or_default();
                active_mode = snapshot.0;
                active_distance = snapshot.1;
                active_elapsed = 0.0;
                current_x = 0;
                current_y = 0;
                edge_origin = None;
                edge_bounds = None;
                random.burst_elapsed = 0.0;
                if active_mode == MovementMode::Realistic {
                    pause.restart();
                }

                if active_distance == MovementDistance::EdgeToEdge {
                    activate_edge_to_edge(
                        mover.as_ref(),
                        &mut active_distance,
                        &mut edge_bounds,
                        &mut edge_origin,
                        &mut edge_note_sent,
                        &tx,
                    );
                }
                if active_distance == MovementDistance::Random {
                    random.reroll();
                }

                was_running = true;
            }

            // In Realistic mode the pause scheduler decides whether this tick
            // moves or holds the cursor; Simple mode always moves.
            let moving = if active_mode == MovementMode::Realistic {
                pause.update(STEP_SEC)
            } else {
                true
            };

            let step_result = if moving {
                active_elapsed += STEP_SEC;
                let cursor = mover.as_mut().expect("cursor mover should exist");
                match active_distance {
                    MovementDistance::Default => step_default(
                        cursor,
                        active_mode,
                        active_elapsed,
                        &mut current_x,
                        &mut current_y,
                    ),
                    MovementDistance::EdgeToEdge => {
                        // bounds/origin are guaranteed Some here: activation only
                        // keeps EdgeToEdge when both are available, otherwise it
                        // falls back to Default.
                        let bounds = edge_bounds.expect("edge bounds set on activation");
                        let origin_y = edge_origin.expect("edge origin set on activation").1;
                        let (x, y) = edge_position(active_mode, bounds, origin_y, active_elapsed);
                        cursor.move_absolute(x, y)
                    }
                    MovementDistance::Random => step_random(
                        cursor,
                        active_mode,
                        active_elapsed,
                        &mut current_x,
                        &mut current_y,
                        &mut random,
                    ),
                }
            } else {
                Ok(())
            };

            if let Err(error) = step_result {
                running.store(false, Ordering::SeqCst);
                mover = None;
                let _ = tx.send(AppEvent::Error(error));
            }

            thread::sleep(STEP);
        }
    });
}

fn restore_cursor(
    cursor: &mut CursorMover,
    distance: MovementDistance,
    edge_origin: Option<(i32, i32)>,
    current_x: i32,
    current_y: i32,
) {
    match distance {
        MovementDistance::EdgeToEdge => {
            if let Some((x, y)) = edge_origin {
                let _ = cursor.move_absolute(x, y);
            }
        }
        MovementDistance::Default | MovementDistance::Random => {
            if current_x != 0 || current_y != 0 {
                let _ = cursor.move_relative(-current_x, -current_y);
            }
        }
    }
}

fn activate_edge_to_edge(
    mover: Option<&CursorMover>,
    active_distance: &mut MovementDistance,
    edge_bounds: &mut Option<Bounds>,
    edge_origin: &mut Option<(i32, i32)>,
    edge_note_sent: &mut bool,
    tx: &Sender<AppEvent>,
) {
    let bounds = screens::virtual_screen_bounds();
    let origin = screens::cursor_position();
    let available = mover
        .map(|cursor| cursor.supports_absolute())
        .unwrap_or(false)
        && bounds.map(edge_bounds_supported).unwrap_or(false)
        && origin.is_some();

    if available {
        *edge_bounds = bounds;
        *edge_origin = origin;
    } else {
        // Absolute positioning is unavailable on this backend (e.g. Wayland
        // with ydotool). Behave as Default and say so once per activation.
        *active_distance = MovementDistance::Default;
        if !*edge_note_sent {
            let _ = tx.send(AppEvent::Status(
                "Edge-to-Edge is unavailable on this system; using Default.".to_string(),
            ));
            *edge_note_sent = true;
        }
    }
}

fn edge_bounds_supported(bounds: Bounds) -> bool {
    // enigo's X11 absolute moves reject negative or non-i16 coordinates, so a
    // virtual desktop wider/taller than i16::MAX cannot be driven there.
    #[cfg(target_os = "linux")]
    {
        bounds.max_x <= i32::from(i16::MAX) && bounds.max_y <= i32::from(i16::MAX)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = bounds;
        true
    }
}

fn step_default(
    cursor: &mut CursorMover,
    mode: MovementMode,
    elapsed: f64,
    current_x: &mut i32,
    current_y: &mut i32,
) -> Result<(), String> {
    let (target_x, target_y) = target_position(mode, elapsed);
    let delta_x = target_x - *current_x;
    let delta_y = target_y - *current_y;
    if delta_x == 0 && delta_y == 0 {
        return Ok(());
    }
    cursor.move_relative(delta_x, delta_y)?;
    *current_x = target_x;
    *current_y = target_y;
    Ok(())
}

fn step_random(
    cursor: &mut CursorMover,
    mode: MovementMode,
    elapsed: f64,
    current_x: &mut i32,
    current_y: &mut i32,
    random: &mut RandomState,
) -> Result<(), String> {
    random.burst_elapsed += STEP_SEC;

    let step = (f64::from(random.dir) * RANDOM_SPEED_PX_PER_SEC * STEP_SEC).round() as i32;
    let desired = *current_x + step;
    let clamped = desired.clamp(-RANDOM_RADIUS_X, RANDOM_RADIUS_X);
    let hit_edge = clamped != desired;
    let delta_x = clamped - *current_x;

    let wobble_y = match mode {
        MovementMode::Simple => 0,
        MovementMode::Realistic => {
            ((elapsed * RANDOM_WOBBLE_FREQ).sin() * RANDOM_REALISTIC_WOBBLE_Y).round() as i32
        }
    };
    let delta_y = wobble_y - *current_y;

    if delta_x != 0 || delta_y != 0 {
        cursor.move_relative(delta_x, delta_y)?;
    }
    *current_x = clamped;
    *current_y = wobble_y;

    if hit_edge || random.burst_elapsed >= random.duration {
        random.reroll();
        random.burst_elapsed = 0.0;
    }

    Ok(())
}

fn edge_position(mode: MovementMode, bounds: Bounds, origin_y: i32, elapsed: f64) -> (i32, i32) {
    // Triangle wave across the full width at a constant px/sec speed. Note:
    // the sweep follows the screens' bounding box, which can include empty
    // desktop space when monitors are staggered — we clamp to the box.
    let span = f64::from(bounds.max_x - bounds.min_x);
    let one_way = (span / EDGE_SPEED_PX_PER_SEC).max(0.001);
    let cycle = one_way * 2.0;
    let t = (elapsed % cycle) / one_way;
    let ping = if t <= 1.0 { t } else { 2.0 - t };
    let x = f64::from(bounds.min_x) + ping * span;
    let abs_x = bounds.clamp_x(x.round() as i32);

    let base_y = bounds.clamp_y(origin_y);
    let abs_y = match mode {
        MovementMode::Simple => base_y,
        MovementMode::Realistic => {
            let wobble = (elapsed * EDGE_WOBBLE_FREQ).sin() * EDGE_REALISTIC_WOBBLE_Y;
            bounds.clamp_y(base_y + wobble.round() as i32)
        }
    };

    (abs_x, abs_y)
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

    /// Whether this backend can place the cursor at absolute screen
    /// coordinates. On Linux the `Enigo` variant is only ever chosen on X11
    /// (Wayland uses `Ydotool`), so `Enigo` always implies absolute support.
    fn supports_absolute(&self) -> bool {
        match self {
            Self::Enigo(_) => true,
            #[cfg(target_os = "linux")]
            Self::Ydotool => false,
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

    #[cfg(target_os = "windows")]
    fn move_absolute(&mut self, x: i32, y: i32) -> Result<(), String> {
        // enigo's Windows absolute move normalises against the primary screen
        // only (no MOUSEEVENTF_VIRTUALDESK), so use SetCursorPos directly to
        // reach every monitor, including negative origins. SetCursorPos returns
        // windows::core::Result<()> in windows-rs 0.62.
        unsafe { windows::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y) }
            .map_err(|err| format!("Could not move cursor (SetCursorPos failed): {err}"))
    }

    #[cfg(target_os = "macos")]
    fn move_absolute(&mut self, x: i32, y: i32) -> Result<(), String> {
        // CoreGraphics top-left global coordinates, consistent with the origin
        // captured in screens.rs (and unlike enigo's flipped NSEvent location).
        core_graphics::display::CGDisplay::warp_mouse_cursor_position(
            core_graphics::geometry::CGPoint::new(f64::from(x), f64::from(y)),
        )
        .map_err(|err| format!("Could not move cursor: {err:?}"))
    }

    #[cfg(target_os = "linux")]
    fn move_absolute(&mut self, x: i32, y: i32) -> Result<(), String> {
        match self {
            Self::Enigo(enigo) => enigo
                .move_mouse(x, y, Coordinate::Abs)
                .map_err(|err| format!("Could not move cursor: {err}")),
            Self::Ydotool => {
                Err("Absolute cursor positioning is unavailable on Wayland.".to_string())
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    fn move_absolute(&mut self, _x: i32, _y: i32) -> Result<(), String> {
        Err("Absolute cursor positioning is unsupported on this platform.".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BOUNDS: Bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 1919,
        max_y: 1079,
    };

    #[test]
    fn default_target_position_matches_golden_samples() {
        // Pin the original behaviour so refactors can't drift it.
        assert_eq!(target_position(MovementMode::Simple, 0.0), (0, 0));
        assert_eq!(target_position(MovementMode::Simple, 1.0), (240, 0));
        // Realistic at t=0 is offset by its constant-phase harmonics.
        assert_eq!(target_position(MovementMode::Realistic, 0.0), (14, 29));
    }

    #[test]
    fn edge_position_stays_within_bounds_and_reaches_edges() {
        let bounds = SAMPLE_BOUNDS;
        // Sweep long enough to cover several full traversals. Sampling is
        // coarse relative to the triangle peak, so assert we get within a few
        // pixels of each edge rather than landing on it exactly.
        let mut min_seen = bounds.max_x;
        let mut max_seen = bounds.min_x;
        let steps = 20_000;
        for i in 0..steps {
            let elapsed = i as f64 * 0.01;
            let (x, y) = edge_position(MovementMode::Simple, bounds, 540, elapsed);
            assert!(
                x >= bounds.min_x && x <= bounds.max_x,
                "x={x} out of bounds"
            );
            assert_eq!(y, 540, "Simple edge-to-edge stays on the baseline row");
            min_seen = min_seen.min(x);
            max_seen = max_seen.max(x);
        }
        assert!(
            min_seen <= bounds.min_x + 2,
            "sweep never reached the left edge (min seen {min_seen})"
        );
        assert!(
            max_seen >= bounds.max_x - 2,
            "sweep never reached the right edge (max seen {max_seen})"
        );
    }

    #[test]
    fn edge_position_realistic_clamps_vertical_wobble() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 100,
            max_y: 20,
        };
        // Origin near the top so un-clamped wobble would leave the screen.
        for i in 0..5_000 {
            let elapsed = i as f64 * 0.01;
            let (_x, y) = edge_position(MovementMode::Realistic, bounds, 2, elapsed);
            assert!(
                y >= bounds.min_y && y <= bounds.max_y,
                "y={y} out of bounds"
            );
        }
    }

    #[test]
    fn random_reroll_produces_in_range_duration_and_direction() {
        let mut random = RandomState::new();
        for _ in 0..1_000 {
            random.reroll();
            assert!(random.dir == 1 || random.dir == -1);
            assert!((RANDOM_MIN_DURATION_SEC..=RANDOM_MAX_DURATION_SEC).contains(&random.duration));
        }
    }

    #[test]
    fn edge_bounds_supported_rejects_oversized_on_linux() {
        let ok = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 100,
            max_y: 100,
        };
        assert!(edge_bounds_supported(ok));

        let huge = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: i32::from(i16::MAX) + 1,
            max_y: 100,
        };
        if cfg!(target_os = "linux") {
            assert!(!edge_bounds_supported(huge));
        } else {
            assert!(edge_bounds_supported(huge));
        }
    }

    #[test]
    fn pause_durations_stay_in_range() {
        for _ in 0..1_000 {
            assert!((PAUSE_MOVE_MIN_SEC..=PAUSE_MOVE_MAX_SEC).contains(&random_moving_duration()));
            assert!((PAUSE_MIN_SEC..=PAUSE_MAX_SEC).contains(&random_pause_duration()));
        }
    }

    #[test]
    fn pause_starts_moving_then_alternates_and_always_resumes() {
        let mut pause = PauseState::new();
        pause.restart();
        assert_eq!(
            pause.phase,
            PausePhase::Moving,
            "must start in a moving phase"
        );

        let dt = 0.05_f64;
        let mut saw_pause = false;
        let mut saw_resume_after_pause = false;
        let mut longest_pause = 0.0_f64;
        let mut current_pause = 0.0_f64;

        // Simulate ~400s of wall time: many move/pause cycles.
        for _ in 0..8_000 {
            if pause.update(dt) {
                if saw_pause {
                    saw_resume_after_pause = true;
                }
                longest_pause = longest_pause.max(current_pause);
                current_pause = 0.0;
            } else {
                saw_pause = true;
                current_pause += dt;
            }
        }
        longest_pause = longest_pause.max(current_pause);

        assert!(saw_pause, "Realistic mode never paused");
        assert!(
            saw_resume_after_pause,
            "movement did not resume after a pause"
        );
        assert!(
            longest_pause <= PAUSE_MAX_SEC + dt,
            "a pause ran longer than the configured maximum: {longest_pause}"
        );
    }
}
