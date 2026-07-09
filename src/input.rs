use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use crate::{
    config::{AppConfig, Binding, BindingKind},
    events::AppEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindTarget {
    Start,
    Stop,
}

const CAPTURED_BINDING_SUPPRESS: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
pub struct CaptureRequest {
    pub target: BindTarget,
}

#[derive(Debug, Default)]
pub struct CaptureState {
    pub request: Option<CaptureRequest>,
    suppress_until: Option<Instant>,
}

pub type SharedCaptureTarget = Arc<Mutex<CaptureState>>;

pub fn spawn_input_listener(
    tx: Sender<AppEvent>,
    running: Arc<AtomicBool>,
    config: Arc<RwLock<AppConfig>>,
    capture_target: SharedCaptureTarget,
) {
    platform::spawn_input_listener(tx, running, config, capture_target);
}

fn handle_binding(
    binding: Binding,
    tx: &Sender<AppEvent>,
    running: &Arc<AtomicBool>,
    config: &Arc<RwLock<AppConfig>>,
    capture_target: &SharedCaptureTarget,
) {
    if let Ok(mut state) = capture_target.lock() {
        if binding.is_left_click() {
            let is_capturing = state.request.is_some();
            drop(state);
            if is_capturing {
                let _ = tx.send(AppEvent::Status(
                    "Left click cannot be used as a binding.".to_string(),
                ));
            }
            return;
        }

        if let Some(request) = state.request {
            state.request = None;

            if binding.label == "Escape" {
                drop(state);
                let _ = tx.send(AppEvent::Status("Binding capture cancelled.".to_string()));
            } else {
                state.suppress_until = Some(Instant::now() + CAPTURED_BINDING_SUPPRESS);
                drop(state);
                let _ = tx.send(AppEvent::BindingCaptured(request.target, binding));
            }
            return;
        }

        if input_is_suppressed(&mut state) {
            return;
        }
    }

    let Ok(config) = config.read() else {
        return;
    };

    if config.start_binding.as_ref() == Some(&binding) {
        running.store(true, Ordering::SeqCst);
        let _ = tx.send(AppEvent::Status("Running.".to_string()));
    } else if config.stop_binding.as_ref() == Some(&binding) {
        running.store(false, Ordering::SeqCst);
        let _ = tx.send(AppEvent::Status("Stopped.".to_string()));
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Modifiers {
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

fn chord_binding(
    kind: BindingKind,
    code_prefix: &str,
    code: u32,
    label: String,
    modifiers: Modifiers,
) -> Binding {
    let mut codes = Vec::with_capacity(4);
    let mut labels = Vec::with_capacity(5);

    for (pressed, code, label) in [
        (modifiers.ctrl, "ctrl", "Ctrl"),
        (modifiers.alt, "alt", "Alt"),
        (modifiers.shift, "shift", "Shift"),
        (modifiers.meta, "meta", "Meta"),
    ] {
        if pressed {
            codes.push(code);
            labels.push(label.to_string());
        }
    }

    labels.push(label);
    let code = if codes.is_empty() {
        format!("{code_prefix}:{code}")
    } else {
        format!("{code_prefix}:{}+{code}", codes.join("+"))
    };

    Binding {
        kind,
        code,
        label: labels.join("+"),
    }
}

fn input_is_suppressed(state: &mut CaptureState) -> bool {
    let Some(until) = state.suppress_until else {
        return false;
    };

    if Instant::now() >= until {
        state.suppress_until = None;
        return false;
    }

    true
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{
        fs::File,
        io::Read,
        os::unix::fs::FileTypeExt,
        path::PathBuf,
        sync::{atomic::AtomicBool, mpsc::Sender, Arc, RwLock},
        thread,
    };

    use super::{
        chord_binding, handle_binding, AppConfig, AppEvent, Binding, BindingKind, Modifiers,
        SharedCaptureTarget,
    };

    const EV_KEY: u16 = 0x01;
    const KEY_RELEASE: i32 = 0;
    const KEY_PRESS: i32 = 1;

    #[cfg(target_pointer_width = "64")]
    const INPUT_EVENT_SIZE: usize = 24;
    #[cfg(target_pointer_width = "64")]
    const EVENT_TYPE_OFFSET: usize = 16;

    #[cfg(target_pointer_width = "32")]
    const INPUT_EVENT_SIZE: usize = 16;
    #[cfg(target_pointer_width = "32")]
    const EVENT_TYPE_OFFSET: usize = 8;

    pub fn spawn_input_listener(
        tx: Sender<AppEvent>,
        running: Arc<AtomicBool>,
        config: Arc<RwLock<AppConfig>>,
        capture_target: SharedCaptureTarget,
    ) {
        thread::spawn(move || {
            let devices = discover_input_devices();
            if devices.is_empty() {
                let _ = tx.send(AppEvent::Error(
                    "No Linux input event devices were found for binding capture.".to_string(),
                ));
                return;
            }

            let mut opened = 0usize;
            for device in devices {
                let tx = tx.clone();
                let running = Arc::clone(&running);
                let config = Arc::clone(&config);
                let capture_target = Arc::clone(&capture_target);

                match File::open(&device.path) {
                    Ok(file) => {
                        opened += 1;
                        thread::spawn(move || {
                            read_device_loop(device, file, tx, running, config, capture_target)
                        });
                    }
                    Err(error) => {
                        let _ = tx.send(AppEvent::Status(format!(
                            "Skipping {}: {error}",
                            device.path.display()
                        )));
                    }
                }
            }

            if opened == 0 {
                let _ = tx.send(AppEvent::Error(
                    "Could not open any Linux input event devices. Add this user to the input group or adjust device permissions."
                        .to_string(),
                ));
            } else {
                let _ = tx.send(AppEvent::Status(format!(
                    "Listening for bindings on {opened} Linux input devices."
                )));
            }
        });
    }

    #[derive(Debug, Clone)]
    struct InputDevice {
        path: PathBuf,
        name: String,
    }

    fn discover_input_devices() -> Vec<InputDevice> {
        let Ok(contents) = std::fs::read_to_string("/proc/bus/input/devices") else {
            return Vec::new();
        };

        contents
            .split("\n\n")
            .filter_map(|block| {
                let name = extract_quoted_field(block, "N: Name=")?;
                if name
                    .to_ascii_lowercase()
                    .contains("ydotoold virtual device")
                {
                    return None;
                }

                let handlers = extract_line_value(block, "H: Handlers=")?;
                let event = handlers
                    .split_whitespace()
                    .find(|handler| handler.starts_with("event"))?;
                let has_keyboard_or_mouse = handlers
                    .split_whitespace()
                    .any(|handler| handler == "kbd" || handler.starts_with("mouse"));
                let has_key_events = extract_line_value(block, "B: EV=")
                    .map(|value| {
                        value.contains('3') || value.contains("17") || value.contains("1f")
                    })
                    .unwrap_or(false);

                if !has_keyboard_or_mouse && !has_key_events {
                    return None;
                }

                Some(InputDevice {
                    path: PathBuf::from("/dev/input").join(event),
                    name,
                })
            })
            .filter(|device| {
                std::fs::metadata(&device.path)
                    .map(|metadata| metadata.file_type().is_char_device())
                    .unwrap_or(false)
            })
            .collect()
    }

    fn extract_quoted_field(block: &str, prefix: &str) -> Option<String> {
        let value = extract_line_value(block, prefix)?;
        value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .map(ToOwned::to_owned)
    }

    fn extract_line_value<'a>(block: &'a str, prefix: &str) -> Option<&'a str> {
        block
            .lines()
            .find_map(|line| line.strip_prefix(prefix).map(str::trim))
    }

    fn read_device_loop(
        device: InputDevice,
        mut file: File,
        tx: Sender<AppEvent>,
        running: Arc<AtomicBool>,
        config: Arc<RwLock<AppConfig>>,
        capture_target: SharedCaptureTarget,
    ) {
        let mut buffer = [0u8; INPUT_EVENT_SIZE];
        let mut pressed_modifiers = PressedModifiers::default();

        loop {
            if let Err(error) = file.read_exact(&mut buffer) {
                let _ = tx.send(AppEvent::Status(format!(
                    "Stopped reading {} ({}): {error}",
                    device.path.display(),
                    device.name
                )));
                return;
            }

            let event_type =
                u16::from_ne_bytes([buffer[EVENT_TYPE_OFFSET], buffer[EVENT_TYPE_OFFSET + 1]]);
            let code =
                u16::from_ne_bytes([buffer[EVENT_TYPE_OFFSET + 2], buffer[EVENT_TYPE_OFFSET + 3]]);
            let value = i32::from_ne_bytes([
                buffer[EVENT_TYPE_OFFSET + 4],
                buffer[EVENT_TYPE_OFFSET + 5],
                buffer[EVENT_TYPE_OFFSET + 6],
                buffer[EVENT_TYPE_OFFSET + 7],
            ]);

            if event_type != EV_KEY {
                continue;
            }

            if pressed_modifiers.update(code, value != KEY_RELEASE) {
                continue;
            }

            if value == KEY_PRESS {
                if let Some(binding) = linux_binding(code, pressed_modifiers.current()) {
                    handle_binding(binding, &tx, &running, &config, &capture_target);
                }
            }
        }
    }

    #[derive(Default)]
    struct PressedModifiers(u8);

    impl PressedModifiers {
        fn update(&mut self, code: u16, pressed: bool) -> bool {
            let bit = match code {
                29 => 1,
                97 => 2,
                56 => 4,
                100 => 8,
                42 => 16,
                54 => 32,
                125 => 64,
                126 => 128,
                _ => return false,
            };

            if pressed {
                self.0 |= bit;
            } else {
                self.0 &= !bit;
            }
            true
        }

        fn current(&self) -> Modifiers {
            Modifiers {
                ctrl: self.0 & 3 != 0,
                alt: self.0 & 12 != 0,
                shift: self.0 & 48 != 0,
                meta: self.0 & 192 != 0,
            }
        }
    }

    fn linux_binding(code: u16, modifiers: Modifiers) -> Option<Binding> {
        if let Some(label) = mouse_button_label(code) {
            return Some(chord_binding(
                BindingKind::MouseButton,
                "linux-mouse",
                code.into(),
                label,
                modifiers,
            ));
        }

        Some(chord_binding(
            BindingKind::Key,
            "linux-key",
            code.into(),
            key_label(code)?,
            modifiers,
        ))
    }

    fn mouse_button_label(code: u16) -> Option<String> {
        match code {
            272 => Some("Mouse Left".to_string()),
            273 => Some("Mouse Right".to_string()),
            274 => Some("Mouse Middle".to_string()),
            275 => Some("Mouse Side".to_string()),
            276 => Some("Mouse Extra".to_string()),
            277 => Some("Mouse Forward".to_string()),
            278 => Some("Mouse Back".to_string()),
            279 => Some("Mouse Task".to_string()),
            280..=287 => Some(format!("Mouse Button {}", code - 271)),
            _ => None,
        }
    }

    fn key_label(code: u16) -> Option<String> {
        let label = match code {
            1 => "Escape",
            2 => "1",
            3 => "2",
            4 => "3",
            5 => "4",
            6 => "5",
            7 => "6",
            8 => "7",
            9 => "8",
            10 => "9",
            11 => "0",
            12 => "-",
            13 => "=",
            14 => "Backspace",
            15 => "Tab",
            16 => "Q",
            17 => "W",
            18 => "E",
            19 => "R",
            20 => "T",
            21 => "Y",
            22 => "U",
            23 => "I",
            24 => "O",
            25 => "P",
            26 => "[",
            27 => "]",
            28 => "Enter",
            29 => "Left Ctrl",
            30 => "A",
            31 => "S",
            32 => "D",
            33 => "F",
            34 => "G",
            35 => "H",
            36 => "J",
            37 => "K",
            38 => "L",
            39 => ";",
            40 => "'",
            41 => "`",
            42 => "Left Shift",
            43 => "\\",
            44 => "Z",
            45 => "X",
            46 => "C",
            47 => "V",
            48 => "B",
            49 => "N",
            50 => "M",
            51 => ",",
            52 => ".",
            53 => "/",
            54 => "Right Shift",
            56 => "Left Alt",
            57 => "Space",
            58 => "Caps Lock",
            59 => "F1",
            60 => "F2",
            61 => "F3",
            62 => "F4",
            63 => "F5",
            64 => "F6",
            65 => "F7",
            66 => "F8",
            67 => "F9",
            68 => "F10",
            87 => "F11",
            88 => "F12",
            97 => "Right Ctrl",
            100 => "Right Alt",
            102 => "Home",
            103 => "Up Arrow",
            104 => "Page Up",
            105 => "Left Arrow",
            106 => "Right Arrow",
            107 => "End",
            108 => "Down Arrow",
            109 => "Page Down",
            110 => "Insert",
            111 => "Delete",
            125 => "Left Meta",
            126 => "Right Meta",
            _ => return None,
        };

        Some(label.to_string())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::{
        sync::{atomic::AtomicBool, mpsc::Sender, Arc, Mutex, OnceLock, RwLock},
        thread,
    };

    use windows::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        UI::WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT,
            MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
            WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
        },
    };

    use super::{
        chord_binding, handle_binding, AppConfig, AppEvent, Binding, BindingKind, Modifiers,
        SharedCaptureTarget,
    };

    static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

    struct HookState {
        tx: Sender<AppEvent>,
        running: Arc<AtomicBool>,
        config: Arc<RwLock<AppConfig>>,
        capture_target: SharedCaptureTarget,
        pressed_modifiers: PressedModifiers,
    }

    pub fn spawn_input_listener(
        tx: Sender<AppEvent>,
        running: Arc<AtomicBool>,
        config: Arc<RwLock<AppConfig>>,
        capture_target: SharedCaptureTarget,
    ) {
        thread::spawn(move || {
            let error_tx = tx.clone();
            if HOOK_STATE
                .set(Mutex::new(HookState {
                    tx,
                    running,
                    config,
                    capture_target,
                    pressed_modifiers: PressedModifiers::default(),
                }))
                .is_err()
            {
                let _ = error_tx.send(AppEvent::Error(
                    "Windows input listener was already running.".to_string(),
                ));
                return;
            }

            let keyboard_hook =
                match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) } {
                    Ok(hook) => hook,
                    Err(error) => {
                        let _ = error_tx.send(AppEvent::Error(format!(
                            "Could not install Windows keyboard hook: {error}"
                        )));
                        return;
                    }
                };

            let mouse_hook =
                match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) } {
                    Ok(hook) => hook,
                    Err(error) => {
                        let _ = unsafe { UnhookWindowsHookEx(keyboard_hook) };
                        let _ = error_tx.send(AppEvent::Error(format!(
                            "Could not install Windows mouse hook: {error}"
                        )));
                        return;
                    }
                };

            let _ = error_tx.send(AppEvent::Status(
                "Listening for Windows keyboard and mouse bindings.".to_string(),
            ));

            let mut message = MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0).as_bool() } {}

            let _ = unsafe { UnhookWindowsHookEx(keyboard_hook) };
            let _ = unsafe { UnhookWindowsHookEx(mouse_hook) };
        });
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP) {
                let event = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
                dispatch_key(
                    event.vkCode,
                    message == WM_KEYDOWN || message == WM_SYSKEYDOWN,
                );
            }
        }

        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            let button = match message {
                WM_LBUTTONDOWN => Some(1),
                WM_RBUTTONDOWN => Some(2),
                WM_MBUTTONDOWN => Some(3),
                WM_XBUTTONDOWN => {
                    let event = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
                    match (event.mouseData >> 16) & 0xffff {
                        1 => Some(4),
                        2 => Some(5),
                        other => Some(other + 3),
                    }
                }
                _ => None,
            };

            if let Some(button) = button {
                dispatch_mouse(button);
            }
        }

        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    #[derive(Default)]
    struct PressedModifiers(u8);

    impl PressedModifiers {
        fn update(&mut self, vk_code: u32, pressed: bool) -> bool {
            let bit = match vk_code {
                0x11 | 0xA2 => 1,
                0xA3 => 2,
                0x12 | 0xA4 => 4,
                0xA5 => 8,
                0x10 | 0xA0 => 16,
                0xA1 => 32,
                0x5B => 64,
                0x5C => 128,
                _ => return false,
            };

            if pressed {
                self.0 |= bit;
            } else {
                self.0 &= !bit;
            }
            true
        }

        fn current(&self) -> Modifiers {
            Modifiers {
                ctrl: self.0 & 3 != 0,
                alt: self.0 & 12 != 0,
                shift: self.0 & 48 != 0,
                meta: self.0 & 192 != 0,
            }
        }
    }

    fn dispatch_key(vk_code: u32, pressed: bool) {
        let Some(mut state) = HOOK_STATE.get().and_then(|state| state.lock().ok()) else {
            return;
        };

        if state.pressed_modifiers.update(vk_code, pressed) || !pressed {
            return;
        }

        let binding = key_binding(vk_code, state.pressed_modifiers.current());
        handle_binding(
            binding,
            &state.tx,
            &state.running,
            &state.config,
            &state.capture_target,
        );
    }

    fn dispatch_mouse(button: u32) {
        let Some(state) = HOOK_STATE.get().and_then(|state| state.lock().ok()) else {
            return;
        };

        handle_binding(
            mouse_binding(button, state.pressed_modifiers.current()),
            &state.tx,
            &state.running,
            &state.config,
            &state.capture_target,
        );
    }

    fn key_binding(vk_code: u32, modifiers: Modifiers) -> Binding {
        chord_binding(
            BindingKind::Key,
            "windows-key",
            vk_code,
            windows_key_label(vk_code),
            modifiers,
        )
    }

    fn mouse_binding(button: u32, modifiers: Modifiers) -> Binding {
        chord_binding(
            BindingKind::MouseButton,
            "windows-mouse",
            button,
            mouse_label(button),
            modifiers,
        )
    }

    fn mouse_label(button: u32) -> String {
        match button {
            1 => "Mouse Left".to_string(),
            2 => "Mouse Right".to_string(),
            3 => "Mouse Middle".to_string(),
            4 => "Mouse Button 4".to_string(),
            5 => "Mouse Button 5".to_string(),
            other => format!("Mouse Button {other}"),
        }
    }

    fn windows_key_label(vk_code: u32) -> String {
        match vk_code {
            0x08 => "Backspace".to_string(),
            0x09 => "Tab".to_string(),
            0x0D => "Enter".to_string(),
            0x10 => "Shift".to_string(),
            0x11 => "Ctrl".to_string(),
            0x12 => "Alt".to_string(),
            0x14 => "Caps Lock".to_string(),
            0x1B => "Escape".to_string(),
            0x20 => "Space".to_string(),
            0x21 => "Page Up".to_string(),
            0x22 => "Page Down".to_string(),
            0x23 => "End".to_string(),
            0x24 => "Home".to_string(),
            0x25 => "Left Arrow".to_string(),
            0x26 => "Up Arrow".to_string(),
            0x27 => "Right Arrow".to_string(),
            0x28 => "Down Arrow".to_string(),
            0x2D => "Insert".to_string(),
            0x2E => "Delete".to_string(),
            0x5B => "Left Meta".to_string(),
            0x5C => "Right Meta".to_string(),
            0x70..=0x7B => format!("F{}", vk_code - 0x6F),
            0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk_code)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| format!("VK {vk_code}")),
            other => format!("VK {other}"),
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{
        sync::{atomic::AtomicBool, mpsc::Sender, Arc, RwLock},
        thread,
    };

    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
        CGEventTapPlacement, CGEventType, CallbackResult, EventField, KeyCode,
    };

    use super::{
        chord_binding, handle_binding, AppConfig, AppEvent, Binding, BindingKind, Modifiers,
        SharedCaptureTarget,
    };

    pub fn spawn_input_listener(
        tx: Sender<AppEvent>,
        running: Arc<AtomicBool>,
        config: Arc<RwLock<AppConfig>>,
        capture_target: SharedCaptureTarget,
    ) {
        thread::spawn(move || {
            let status_tx = tx.clone();
            let _ = status_tx.send(AppEvent::Status(
                "Listening for macOS keyboard and mouse bindings.".to_string(),
            ));

            let result = CGEventTap::with_enabled(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                vec![
                    CGEventType::KeyDown,
                    CGEventType::FlagsChanged,
                    CGEventType::LeftMouseDown,
                    CGEventType::RightMouseDown,
                    CGEventType::OtherMouseDown,
                ],
                move |_proxy, event_type, event| {
                    if let Some(binding) = event_binding(event_type, event) {
                        handle_binding(binding, &tx, &running, &config, &capture_target);
                    }

                    CallbackResult::Keep
                },
                CFRunLoop::run_current,
            );

            if result.is_err() {
                let _ = status_tx.send(AppEvent::Error(
                    "Could not create macOS event tap. Grant Accessibility/Input Monitoring permission for this app and restart it.".to_string(),
                ));
            }
        });
    }

    fn event_binding(event_type: CGEventType, event: &CGEvent) -> Option<Binding> {
        match event_type {
            CGEventType::KeyDown => {
                let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                Some(key_binding(code as u16, modifiers(event.get_flags())))
            }
            CGEventType::FlagsChanged => None,
            CGEventType::LeftMouseDown => Some(mouse_binding(0, modifiers(event.get_flags()))),
            CGEventType::RightMouseDown => Some(mouse_binding(1, modifiers(event.get_flags()))),
            CGEventType::OtherMouseDown => {
                let button = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
                Some(mouse_binding(
                    u32::try_from(button).unwrap_or(0),
                    modifiers(event.get_flags()),
                ))
            }
            _ => None,
        }
    }

    fn key_binding(code: u16, modifiers: Modifiers) -> Binding {
        chord_binding(
            BindingKind::Key,
            "mac-key",
            code.into(),
            mac_key_label(code),
            modifiers,
        )
    }

    fn mouse_binding(button: u32, modifiers: Modifiers) -> Binding {
        chord_binding(
            BindingKind::MouseButton,
            "mac-mouse",
            button,
            mouse_label(button),
            modifiers,
        )
    }

    fn modifiers(flags: CGEventFlags) -> Modifiers {
        Modifiers {
            ctrl: flags.contains(CGEventFlags::CGEventFlagControl),
            alt: flags.contains(CGEventFlags::CGEventFlagAlternate),
            shift: flags.contains(CGEventFlags::CGEventFlagShift),
            meta: flags.contains(CGEventFlags::CGEventFlagCommand),
        }
    }

    fn mouse_label(button: u32) -> String {
        match button {
            0 => "Mouse Left".to_string(),
            1 => "Mouse Right".to_string(),
            2 => "Mouse Middle".to_string(),
            3 => "Mouse Button 4".to_string(),
            4 => "Mouse Button 5".to_string(),
            other => format!("Mouse Button {}", other + 1),
        }
    }

    fn mac_key_label(code: u16) -> String {
        match code {
            KeyCode::ANSI_A => "A",
            KeyCode::ANSI_B => "B",
            KeyCode::ANSI_C => "C",
            KeyCode::ANSI_D => "D",
            KeyCode::ANSI_E => "E",
            KeyCode::ANSI_F => "F",
            KeyCode::ANSI_G => "G",
            KeyCode::ANSI_H => "H",
            KeyCode::ANSI_I => "I",
            KeyCode::ANSI_J => "J",
            KeyCode::ANSI_K => "K",
            KeyCode::ANSI_L => "L",
            KeyCode::ANSI_M => "M",
            KeyCode::ANSI_N => "N",
            KeyCode::ANSI_O => "O",
            KeyCode::ANSI_P => "P",
            KeyCode::ANSI_Q => "Q",
            KeyCode::ANSI_R => "R",
            KeyCode::ANSI_S => "S",
            KeyCode::ANSI_T => "T",
            KeyCode::ANSI_U => "U",
            KeyCode::ANSI_V => "V",
            KeyCode::ANSI_W => "W",
            KeyCode::ANSI_X => "X",
            KeyCode::ANSI_Y => "Y",
            KeyCode::ANSI_Z => "Z",
            KeyCode::ANSI_0 => "0",
            KeyCode::ANSI_1 => "1",
            KeyCode::ANSI_2 => "2",
            KeyCode::ANSI_3 => "3",
            KeyCode::ANSI_4 => "4",
            KeyCode::ANSI_5 => "5",
            KeyCode::ANSI_6 => "6",
            KeyCode::ANSI_7 => "7",
            KeyCode::ANSI_8 => "8",
            KeyCode::ANSI_9 => "9",
            KeyCode::RETURN => "Enter",
            KeyCode::TAB => "Tab",
            KeyCode::SPACE => "Space",
            KeyCode::DELETE => "Backspace",
            KeyCode::ESCAPE => "Escape",
            KeyCode::COMMAND => "Left Meta",
            KeyCode::RIGHT_COMMAND => "Right Meta",
            KeyCode::SHIFT => "Left Shift",
            KeyCode::RIGHT_SHIFT => "Right Shift",
            KeyCode::OPTION => "Left Alt",
            KeyCode::RIGHT_OPTION => "Right Alt",
            KeyCode::CONTROL => "Left Ctrl",
            KeyCode::RIGHT_CONTROL => "Right Ctrl",
            KeyCode::CAPS_LOCK => "Caps Lock",
            KeyCode::HOME => "Home",
            KeyCode::END => "End",
            KeyCode::PAGE_UP => "Page Up",
            KeyCode::PAGE_DOWN => "Page Down",
            KeyCode::LEFT_ARROW => "Left Arrow",
            KeyCode::RIGHT_ARROW => "Right Arrow",
            KeyCode::UP_ARROW => "Up Arrow",
            KeyCode::DOWN_ARROW => "Down Arrow",
            KeyCode::F1 => "F1",
            KeyCode::F2 => "F2",
            KeyCode::F3 => "F3",
            KeyCode::F4 => "F4",
            KeyCode::F5 => "F5",
            KeyCode::F6 => "F6",
            KeyCode::F7 => "F7",
            KeyCode::F8 => "F8",
            KeyCode::F9 => "F9",
            KeyCode::F10 => "F10",
            KeyCode::F11 => "F11",
            KeyCode::F12 => "F12",
            _ => return format!("Key {code}"),
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicBool, mpsc, Arc, Mutex, RwLock};

    use super::*;

    fn binding(label: &str) -> Binding {
        Binding {
            kind: BindingKind::Key,
            code: format!("test:{label}"),
            label: label.to_string(),
        }
    }

    fn mouse_binding(label: &str) -> Binding {
        Binding {
            kind: BindingKind::MouseButton,
            code: format!("test-mouse:{label}"),
            label: label.to_string(),
        }
    }

    fn armed_capture() -> SharedCaptureTarget {
        Arc::new(Mutex::new(CaptureState {
            request: Some(CaptureRequest {
                target: BindTarget::Start,
            }),
            suppress_until: None,
        }))
    }

    fn deps() -> (
        Sender<AppEvent>,
        mpsc::Receiver<AppEvent>,
        Arc<AtomicBool>,
        Arc<RwLock<AppConfig>>,
    ) {
        let (tx, rx) = mpsc::channel();
        (
            tx,
            rx,
            Arc::new(AtomicBool::new(false)),
            Arc::new(RwLock::new(AppConfig::default())),
        )
    }

    #[test]
    fn capture_rejects_left_click() {
        let (tx, rx, running, config) = deps();
        let capture = armed_capture();

        handle_binding(
            mouse_binding("Mouse Left"),
            &tx,
            &running,
            &config,
            &capture,
        );

        assert!(matches!(rx.try_recv(), Ok(AppEvent::Status(_))));
        assert!(capture.lock().unwrap().request.is_some());
    }

    #[test]
    fn capture_binds_immediate_regular_key() {
        let (tx, rx, running, config) = deps();
        let capture = armed_capture();

        handle_binding(binding("F6"), &tx, &running, &config, &capture);

        match rx.try_recv().unwrap() {
            AppEvent::BindingCaptured(BindTarget::Start, bound) => assert_eq!(bound.label, "F6"),
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(capture.lock().unwrap().request.is_none());
    }

    #[test]
    fn escape_cancels_capture() {
        let (tx, rx, running, config) = deps();
        let capture = armed_capture();

        handle_binding(binding("Escape"), &tx, &running, &config, &capture);

        match rx.try_recv().unwrap() {
            AppEvent::Status(_) => {}
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(capture.lock().unwrap().request.is_none());
    }

    #[test]
    fn captured_binding_does_not_fire_immediately_after_save() {
        let (tx, rx, running, config) = deps();
        let capture = armed_capture();
        let bound = binding("Mouse Middle");

        handle_binding(bound.clone(), &tx, &running, &config, &capture);
        match rx.try_recv().unwrap() {
            AppEvent::BindingCaptured(BindTarget::Start, captured) => {
                config.write().unwrap().start_binding = Some(captured);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        handle_binding(bound, &tx, &running, &config, &capture);

        assert!(!running.load(Ordering::SeqCst));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn left_click_cannot_activate_existing_binding() {
        let (tx, rx, running, config) = deps();
        let capture = Arc::new(Mutex::new(CaptureState::default()));
        config.write().unwrap().start_binding = Some(mouse_binding("Mouse Left"));

        handle_binding(
            mouse_binding("Mouse Left"),
            &tx,
            &running,
            &config,
            &capture,
        );

        assert!(!running.load(Ordering::SeqCst));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn modifier_chord_matches_as_one_binding() {
        let (tx, rx, running, config) = deps();
        let capture = Arc::new(Mutex::new(CaptureState::default()));
        let chord = chord_binding(
            BindingKind::Key,
            "windows-key",
            0x76,
            "F7".to_string(),
            Modifiers {
                ctrl: true,
                alt: true,
                ..Default::default()
            },
        );
        assert_eq!(chord.code, "windows-key:ctrl+alt+118");
        assert_eq!(chord.label, "Ctrl+Alt+F7");
        config.write().unwrap().stop_binding = Some(chord.clone());
        running.store(true, Ordering::SeqCst);

        handle_binding(chord, &tx, &running, &config, &capture);

        assert!(!running.load(Ordering::SeqCst));
        assert!(matches!(rx.try_recv(), Ok(AppEvent::Status(_))));
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use std::sync::{atomic::AtomicBool, mpsc::Sender, Arc, RwLock};

    use super::{AppConfig, AppEvent, SharedCaptureTarget};

    pub fn spawn_input_listener(
        tx: Sender<AppEvent>,
        _running: Arc<AtomicBool>,
        _config: Arc<RwLock<AppConfig>>,
        _capture_target: SharedCaptureTarget,
    ) {
        let _ = tx.send(AppEvent::Error(
            "Global keyboard and mouse bindings are only implemented on Linux, macOS, and Windows."
                .to_string(),
        ));
    }
}
