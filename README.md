# Realistic Mouse Jiggler

Cross-platform desktop mouse jiggler for Linux, macOS, and Windows.

## Features

- Smooth realistic mouse movement, plus a simple horizontal movement mode.
- Native desktop UI with start/stop controls.
- Global keyboard key or mouse button binding for start/stop.
- System tray menu with Open, Start, Stop, and Quit actions.
- Window close/minimize keeps the app running from the tray.
- RMJ icon assets for the window, taskbar, favicon, and tray.

## Install

### Arch / CachyOS

Install the current release package directly:

```bash
sudo pacman -U https://github.com/visorcraft/realistic-mouse-jiggler/releases/download/v0.1.2/realistic-mouse-jiggler-0.1.2-1-x86_64.pkg.tar.zst
```

Or add the GitHub release as a small pacman repository:

```ini
[realistic-mouse-jiggler]
SigLevel = Optional TrustAll
Server = https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download
```

Then install:

```bash
sudo pacman -Sy realistic-mouse-jiggler
```

The package is currently unsigned, which is why the repo stanza uses `Optional TrustAll`.

### Linux Tarball

Linux release tarballs are available on the releases page:

```text
https://github.com/visorcraft/realistic-mouse-jiggler/releases
```

### From Source

Run the app:

```bash
cargo run
```

Build a release binary:

```bash
cargo build --release
```

The binary is written to:

```text
target/release/realistic-mouse-jiggler
```

## Runtime Notes

### Linux

Global mouse/key binding capture reads Linux input devices directly on Wayland. The app needs read access to `/dev/input/event*`; a normal setup is to run as a user in the `input` group.

For cursor movement on Wayland, the app prefers `ydotool` when it is installed:

```bash
systemctl --user start ydotool.service
```

Linux tray support uses the freedesktop/KDE StatusNotifierItem protocol through `ksni`. KDE supports this natively. GNOME users may need an AppIndicator/StatusNotifier extension.

### macOS

macOS requires Accessibility/Input Monitoring permission for global input capture and cursor movement:

```text
System Settings -> Privacy & Security -> Accessibility
```

Add the app, or add Terminal while running through `cargo run`.

### Windows

Windows should work without extra system packages. Some security tools may flag global input hooks; allow the app if you want keyboard/mouse bindings to work system-wide.

## Packaging

Build a local Arch/CachyOS package:

```bash
scripts/build-arch-package.sh --syncdeps
```

Install the local package:

```bash
sudo pacman -U dist/arch/realistic-mouse-jiggler-*.pkg.tar.*
```

Build a static pacman repo directory:

```bash
scripts/build-pacman-repo.sh
```

See [CachyOS and Pacman Packaging](docs/cachyos-packaging.md) for hosting and CachyOS submission notes.

## Development Checks

```bash
cargo fmt --check
cargo test
cargo build --release
```

When editing `.github/workflows/release.yml`, also run:

```bash
actionlint .github/workflows/release.yml
```
