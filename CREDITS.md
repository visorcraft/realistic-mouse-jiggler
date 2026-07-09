# Credits and Attribution

## Copyright

Realistic Mouse Jiggler is © VisorCraft LLC and contributors, distributed under the [MIT License](LICENSE).

## Project

Realistic Mouse Jiggler is a cross-platform desktop utility for realistic cursor movement, tray controls, and global start/stop bindings.

Repository: <https://github.com/visorcraft/realistic-mouse-jiggler>

## Direct Rust dependencies

| Crate | Version | License | Project |
| ----- | ------- | ------- | ------- |
| anyhow | 1.0.103 | MIT OR Apache-2.0 | https://github.com/dtolnay/anyhow |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | https://github.com/servo/core-foundation-rs |
| core-graphics | 0.25.0 | MIT OR Apache-2.0 | https://github.com/servo/core-foundation-rs |
| directories | 6.0.0 | MIT OR Apache-2.0 | https://github.com/soc/directories-rs |
| eframe / egui | 0.35.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui |
| enigo | 0.6.1 | MIT | https://github.com/enigo-rs/enigo |
| image | 0.25.10 | MIT OR Apache-2.0 | https://github.com/image-rs/image |
| ksni | 0.3.5 | Unlicense | https://github.com/iovxw/ksni |
| serde | 1.0.228 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| toml | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| tray-icon | 0.24.1 | MIT OR Apache-2.0 | https://github.com/tauri-apps/tray-icon |
| windows | 0.62.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| winresource | 0.1.31 | MIT | https://github.com/BenjaminRi/winresource |

## Optional runtime integration

| Component | License | Project |
| --------- | ------- | ------- |
| ydotool (Wayland cursor backend) | AGPL-3.0-or-later | https://github.com/ReimuNotMoe/ydotool |

## Transitive Rust dependencies

The full machine-generated transitive supplement — every crate, its exact version, and the full text of every distinct license — is bundled in the in-app **Licenses** page and mirrored at [`docs/credits-third-party.md`](docs/credits-third-party.md).

Regenerate it with:

```bash
cargo about generate about.hbs --output-file docs/credits-third-party.md --locked
```

## Reporting attribution gaps

If you find code or assets in this repository that we have failed to credit, please open an issue at <https://github.com/visorcraft/realistic-mouse-jiggler/issues> and we will correct the record.
