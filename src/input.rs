use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc, Mutex, RwLock,
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

pub type SharedCaptureTarget = Arc<Mutex<Option<BindTarget>>>;

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
    if let Some(target) = capture_target
        .lock()
        .ok()
        .and_then(|mut target| target.take())
    {
        let _ = tx.send(AppEvent::BindingCaptured(target, binding));
        return;
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

    use super::{handle_binding, AppConfig, AppEvent, Binding, BindingKind, SharedCaptureTarget};

    const EV_KEY: u16 = 0x01;
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

            if event_type == EV_KEY && value == KEY_PRESS {
                if let Some(binding) = linux_binding(code) {
                    handle_binding(binding, &tx, &running, &config, &capture_target);
                }
            }
        }
    }

    fn linux_binding(code: u16) -> Option<Binding> {
        if let Some(label) = mouse_button_label(code) {
            return Some(Binding {
                kind: BindingKind::MouseButton,
                code: format!("linux-mouse:{code}"),
                label,
            });
        }

        Some(Binding {
            kind: BindingKind::Key,
            code: format!("linux-key:{code}"),
            label: key_label(code)?,
        })
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
            MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN,
            WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
        },
    };

    use super::{handle_binding, AppConfig, AppEvent, Binding, BindingKind, SharedCaptureTarget};

    static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

    struct HookState {
        tx: Sender<AppEvent>,
        running: Arc<AtomicBool>,
        config: Arc<RwLock<AppConfig>>,
        capture_target: SharedCaptureTarget,
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
            if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
                let event = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
                dispatch_binding(key_binding(event.vkCode));
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
                dispatch_binding(mouse_binding(button));
            }
        }

        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    fn dispatch_binding(binding: Binding) {
        let Some(state) = HOOK_STATE.get().and_then(|state| state.lock().ok()) else {
            return;
        };

        handle_binding(
            binding,
            &state.tx,
            &state.running,
            &state.config,
            &state.capture_target,
        );
    }

    fn key_binding(vk_code: u32) -> Binding {
        Binding {
            kind: BindingKind::Key,
            code: format!("windows-key:{vk_code}"),
            label: windows_key_label(vk_code),
        }
    }

    fn mouse_binding(button: u32) -> Binding {
        Binding {
            kind: BindingKind::MouseButton,
            code: format!("windows-mouse:{button}"),
            label: mouse_label(button),
        }
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

    use super::{handle_binding, AppConfig, AppEvent, Binding, BindingKind, SharedCaptureTarget};

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
                Some(key_binding(code as u16))
            }
            CGEventType::FlagsChanged => {
                let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                if modifier_is_pressed(code, event.get_flags()) {
                    Some(key_binding(code))
                } else {
                    None
                }
            }
            CGEventType::LeftMouseDown => Some(mouse_binding(0)),
            CGEventType::RightMouseDown => Some(mouse_binding(1)),
            CGEventType::OtherMouseDown => {
                let button = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
                Some(mouse_binding(u32::try_from(button).unwrap_or(0)))
            }
            _ => None,
        }
    }

    fn key_binding(code: u16) -> Binding {
        Binding {
            kind: BindingKind::Key,
            code: format!("mac-key:{code}"),
            label: mac_key_label(code),
        }
    }

    fn mouse_binding(button: u32) -> Binding {
        Binding {
            kind: BindingKind::MouseButton,
            code: format!("mac-mouse:{button}"),
            label: mouse_label(button),
        }
    }

    fn modifier_is_pressed(code: u16, flags: CGEventFlags) -> bool {
        match code {
            KeyCode::SHIFT | KeyCode::RIGHT_SHIFT => flags.contains(CGEventFlags::CGEventFlagShift),
            KeyCode::CONTROL | KeyCode::RIGHT_CONTROL => {
                flags.contains(CGEventFlags::CGEventFlagControl)
            }
            KeyCode::OPTION | KeyCode::RIGHT_OPTION => {
                flags.contains(CGEventFlags::CGEventFlagAlternate)
            }
            KeyCode::COMMAND | KeyCode::RIGHT_COMMAND => {
                flags.contains(CGEventFlags::CGEventFlagCommand)
            }
            KeyCode::CAPS_LOCK => flags.contains(CGEventFlags::CGEventFlagAlphaShift),
            _ => false,
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
