# Security

This document describes Realistic Mouse Jiggler's security posture and how to
report vulnerabilities.

## Application scope

Realistic Mouse Jiggler is a local desktop utility. It does not run a network
service, open listening ports, or send telemetry.

The app does interact with sensitive desktop surfaces:

- global keyboard and mouse binding capture,
- cursor movement through platform APIs,
- system tray actions,
- optional Wayland cursor movement through `ydotool`.

These capabilities are local-only, but they can still matter for user trust and
platform permissions.

## Platform permissions

### Linux

On Wayland, global binding capture reads `/dev/input/event*`. Use the narrowest
local permission model that works for your system, commonly membership in the
`input` group or a local udev rule. Do not run the app as root just to capture
bindings.

For cursor movement on Wayland, the app prefers `ydotool` when available.
`ydotoold` is a local user service; review your distro's service configuration
before enabling it.

### macOS

macOS may require Accessibility and Input Monitoring permissions for cursor
movement and global binding capture. Grant these only to the app or terminal you
use to launch it.

### Windows

Windows global hooks are used for keyboard and mouse bindings. Security tools
may flag this behavior because it is the same OS capability used by many hotkey
utilities.

## Data and configuration

The app stores local configuration only. It should not store secrets. Treat any
future feature that stores credentials, network endpoints, or account data as a
new security boundary requiring review.

## Dependency security

Direct runtime dependencies include:

| Dependency | Role |
|---|---|
| `eframe` / `egui` | desktop UI |
| `enigo` | cursor movement |
| `ksni` | Linux StatusNotifierItem tray |
| `tray-icon` | macOS / Windows tray |
| `directories` | config paths |
| `serde` / `toml` | config serialization |
| `image` | icon loading |

Keep dependencies current and license-compatible. Prefer existing dependencies
or the Rust standard library over adding new crates.

## Reporting a vulnerability

**Do not file a public GitHub issue, discussion, or pull request for security
problems.** Report privately through **GitHub's private vulnerability reporting**:

1. Go to the repository's **Security** tab.
2. Click **Report a vulnerability**.
3. Fill in the advisory form with the details below.

Please include:

- a description of the issue and its impact,
- step-by-step reproduction steps,
- the Realistic Mouse Jiggler version, OS, and desktop environment,
- relevant configuration, logs, or proof-of-concept details,
- a suggested fix or mitigation, if you have one.

## What to expect

- Acknowledgement within a few days.
- An initial assessment and remediation plan if confirmed.
- Progress updates through the private advisory thread until resolved.
- Credit in the advisory unless you prefer to remain anonymous.

Please give maintainers a reasonable opportunity to ship a fix before public
disclosure.
