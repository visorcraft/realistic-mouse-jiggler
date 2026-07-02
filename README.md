<!-- SPDX-FileCopyrightText: 2026 VisorCraft LLC -->
<!-- SPDX-License-Identifier: MIT -->

<p align="center">
  <img src="assets/icons/rmj-256.png" alt="Realistic Mouse Jiggler logo" width="180" />
</p>

<h1 align="center">Realistic Mouse Jiggler</h1>

<p align="center">
  <b>The realistic desktop mouse jiggler.</b>
  <br />
  Keep sessions awake with natural cursor motion, tray controls, and global start/stop bindings.
  <br />
  Rust core &middot; egui UI &middot; Linux, macOS, Windows &middot; Arch/CachyOS package &middot; no telemetry &middot; MIT.
</p>

<p align="center">
  <a href="https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest"><img src="https://img.shields.io/github/v/release/visorcraft/realistic-mouse-jiggler?sort=semver" alt="Latest release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT" /></a>
  <img src="https://img.shields.io/badge/built%20with-Rust-000000?logo=rust&amp;logoColor=white" alt="Built with Rust" />
  <img src="https://img.shields.io/badge/UI-egui-4f8cff" alt="UI: egui" />
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-333333" alt="Platform: Linux, macOS, Windows" />
</p>

---

## What is Realistic Mouse Jiggler?

Realistic Mouse Jiggler keeps desktop sessions awake by moving the cursor
with natural-looking motion. It is built for people who want a small,
predictable utility instead of a bulky background app.

It is built around four goals:

- **Realistic motion.** Smooth movement is available alongside a simple
  horizontal mode.
- **Fast control.** Start or stop from the app window, the system tray,
  or a global keyboard key or mouse button.
- **Desktop-native behavior.** Closing or minimizing keeps the app
  available from the tray, and KDE/Wayland restore behavior is handled.
- **Simple distribution.** GitHub releases include a Linux tarball,
  signed Windows artifacts, and a signed Arch/CachyOS pacman package.

---

## Try it

Run from source:

```bash
cargo run
```

Release build:

```bash
cargo build --release
```

The binary is written to:

```text
target/release/realistic-mouse-jiggler
```

---

## Install

### Arch / CachyOS

Install directly from the latest signed package:

```bash
curl -fsSL https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download/install-arch.sh | bash
```

Or set up the signed pacman repository manually. First import and locally
trust the VisorCraft package signing key:

```bash
curl -fsSLo /tmp/visorcraft-packages.asc \
  https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download/visorcraft-packages.asc
sudo pacman-key --add /tmp/visorcraft-packages.asc
sudo pacman-key --lsign-key 55B2BE2BCE1FE5E61D39C2863C7B024310156D2E
```

Add the release repository to `/etc/pacman.conf`:

```ini
[realistic-mouse-jiggler]
SigLevel = Required
Server = https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download
```

Then sync and install with pacman:

```bash
sudo pacman -Syu realistic-mouse-jiggler
```

Direct `pacman -U` installs from the latest release URL work after the
key has been imported and locally trusted:

```bash
sudo pacman -U https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download/realistic-mouse-jiggler-x86_64.pkg.tar.zst
```

Pacman must trust the VisorCraft key before it can install the package.
Each `.pkg.tar.zst` asset is published with the matching `.pkg.tar.zst.sig`
file that pacman expects.

### Linux Tarball

Download the latest Linux tarball:

```bash
curl -LO https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download/realistic-mouse-jiggler-linux-x86_64.tar.gz
```

### Windows

Download the latest signed Windows installer:

```text
https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download/realistic-mouse-jiggler.msi
```

Or the signed standalone `.exe`:

```text
https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download/realistic-mouse-jiggler.exe
```

---

## Runtime Notes

### Linux

Global mouse/key binding capture reads Linux input devices directly on
Wayland. The app needs read access to `/dev/input/event*`; a normal
setup is to run as a user in the `input` group.

For cursor movement on Wayland, the app prefers `ydotool` when it is
installed:

```bash
systemctl --user start ydotool.service
```

Linux tray support uses the freedesktop/KDE StatusNotifierItem protocol
through `ksni`. KDE supports this natively. GNOME users may need an
AppIndicator/StatusNotifier extension.

### macOS

macOS requires Accessibility/Input Monitoring permission for global input
capture and cursor movement:

```text
System Settings -> Privacy & Security -> Accessibility
```

Add the app, or add Terminal while running through `cargo run`.

### Windows

Windows should work without extra system packages. Some security tools
may flag global input hooks; allow the app if you want keyboard/mouse
bindings to work system-wide.

---

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

See [CachyOS and Pacman Packaging](docs/cachyos-packaging.md) for
hosting and CachyOS submission notes.

---

## Architecture

- **`src/app.rs`**: egui UI, close/minimize-to-tray behavior, and
  KDE/Wayland restore helpers.
- **`src/input.rs`**: global keyboard and mouse binding capture.
- **`src/jiggler.rs`**: cursor movement worker.
- **`src/tray.rs`**: system tray integration. Linux uses `ksni`;
  macOS and Windows use `tray-icon`.
- **`src/icons.rs`**: embedded PNG icons and Linux desktop/icon fallback.
- **`packaging/arch/`**: Arch/CachyOS package metadata.

---

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
