# Realistic Mouse Jiggler

Cross-platform desktop mouse jiggler for Linux, macOS, and Windows.

## Features

- Smooth realistic mouse movement or simple horizontal movement.
- Start and stop buttons in a small native desktop UI.
- Bind start/stop to a global keyboard key or mouse button.
- System tray icon with Open, Start, Stop, and Quit menu actions.
- Closing or minimizing the window keeps the app available from the tray.

## Build

```bash
cargo run
```

Release build:

```bash
cargo build --release
```

The binary will be at:

```text
target/release/realistic-mouse-jiggler
```

## Linux Notes

The app uses native input APIs where possible.

For global mouse/key binding capture on Wayland, Linux uses a direct `/dev/input/event*` reader instead of stale global-hook crates. The process needs read access to Linux input devices. A normal setup is to run as a user in the `input` group.

For cursor movement on Linux Wayland, the app prefers `ydotool` when it is installed:

```bash
systemctl --user start ydotool.service
```

Linux tray support uses the freedesktop/KDE StatusNotifierItem protocol through `ksni`; it does not use GTK/AppIndicator. KDE supports this natively. GNOME users may need an AppIndicator/StatusNotifier extension.

## macOS Notes

macOS requires Accessibility/Input Monitoring permission for global input capture and cursor movement:

System Settings -> Privacy & Security -> Accessibility

Add the app, or add Terminal while running through `cargo run`.

## Windows Notes

Windows should work without extra system packages. Some security tools may flag global input hooks; allow the app if you want keyboard/mouse bindings to work system-wide.

## Dependency Policy

Direct dependencies are pinned to the latest stable crates.io versions verified during setup:

| Crate | Version | Purpose |
| --- | --- | --- |
| `eframe` | `0.35.0` | Native egui application shell |
| `ksni` | `0.3.5` | Linux StatusNotifierItem tray |
| `tray-icon` | `0.24.1` | Windows/macOS system tray |
| `enigo` | `0.6.1` | Cross-platform cursor movement |
| `windows` | `0.62.2` | Windows keyboard/mouse hook APIs |
| `core-graphics` | `0.25.0` | macOS event taps for bindings |
| `core-foundation` | `0.10.1` | macOS run loop integration |
| `directories` | `6.0.0` | Platform config paths |
| `toml` | `1.1.2` | Config file format |
| `anyhow` | `1.0.103` | Error handling |
| `serde` | `1.0.228` | Config serialization |

Avoid reintroducing `rdev`: it was intentionally removed because its latest release is from 2023 and its Linux listener path is X11-oriented unless unstable evdev grabbing is used. `device_query` was also removed because its macOS listener path pulls older helper crates. `keytap`, `hotkey-listener`, and `global-hotkey` were considered but do not cover arbitrary mouse-button binding. `uiohook-rs` was rejected because it would make this app GPL-licensed. `raw-input` was rejected because it is alpha. `openlogi-hook` was rejected because Windows support is currently a stub.

`tray-icon` is target-scoped to non-Linux builds with default features disabled, so it does not pull GTK/AppIndicator/libxdo on Linux. Linux uses `ksni` instead because it is current and avoids the deprecated AppIndicator backend.
