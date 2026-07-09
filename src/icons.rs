use eframe::egui::IconData;
use image::ImageFormat;
#[cfg(target_os = "linux")]
use std::path::Path;

#[cfg(target_os = "linux")]
const APP_ID: &str = "com.visorcraft.realistic-mouse-jiggler";
#[cfg(target_os = "linux")]
const APP_NAME: &str = "Realistic Mouse Jiggler";
#[cfg(target_os = "linux")]
const SYSTEM_DESKTOP_PATH: &str =
    "/usr/share/applications/com.visorcraft.realistic-mouse-jiggler.desktop";
#[cfg(target_os = "linux")]
const SYSTEM_ICON_PATH: &str =
    "/usr/share/icons/hicolor/256x256/apps/com.visorcraft.realistic-mouse-jiggler.png";

#[cfg(target_os = "linux")]
pub const RMJ_16_PNG: &[u8] = include_bytes!("../assets/icons/rmj-16.png");
#[cfg(target_os = "linux")]
pub const RMJ_24_PNG: &[u8] = include_bytes!("../assets/icons/rmj-24.png");
pub const RMJ_32_PNG: &[u8] = include_bytes!("../assets/icons/rmj-32.png");
#[cfg(target_os = "linux")]
pub const RMJ_48_PNG: &[u8] = include_bytes!("../assets/icons/rmj-48.png");
#[cfg(target_os = "linux")]
pub const RMJ_64_PNG: &[u8] = include_bytes!("../assets/icons/rmj-64.png");
pub const RMJ_128_PNG: &[u8] = include_bytes!("../assets/icons/rmj-128.png");
pub const RMJ_256_PNG: &[u8] = include_bytes!("../assets/icons/rmj-256.png");

pub struct RgbaIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn window_icon() -> IconData {
    decode_png(RMJ_256_PNG)
        .expect("embedded RMJ window icon should be valid")
        .into()
}

#[cfg(target_os = "linux")]
pub fn install_linux_desktop_icon() {
    if linux_desktop_icon_is_installed() {
        return;
    }

    if let Some(base_dirs) = directories::BaseDirs::new() {
        let data_dir = base_dirs.data_dir();
        let _ = install_linux_desktop_icon_at(data_dir);
    }
}

#[cfg(target_os = "linux")]
fn linux_desktop_icon_is_installed() -> bool {
    Path::new(SYSTEM_DESKTOP_PATH).is_file() && Path::new(SYSTEM_ICON_PATH).is_file()
}

#[cfg(target_os = "linux")]
fn install_linux_desktop_icon_at(data_dir: &Path) -> std::io::Result<()> {
    let icon_dir = data_dir.join("icons/hicolor/256x256/apps");
    let app_dir = data_dir.join("applications");
    let icon_path = icon_dir.join(format!("{APP_ID}.png"));
    let desktop_path = app_dir.join(format!("{APP_ID}.desktop"));
    let exec_path = std::env::current_exe()?;

    std::fs::create_dir_all(&icon_dir)?;
    std::fs::create_dir_all(&app_dir)?;
    std::fs::write(icon_path, RMJ_256_PNG)?;
    std::fs::write(
        desktop_path,
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name={APP_NAME}\n\
             Comment=Cross-platform desktop mouse jiggler\n\
             Exec={}\n\
             Icon={APP_ID}\n\
             Terminal=false\n\
             Categories=Utility;\n\
             StartupWMClass={APP_ID}\n",
            desktop_exec_path(&exec_path),
        ),
    )?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn desktop_exec_path(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

pub fn decode_png(bytes: &[u8]) -> anyhow::Result<RgbaIcon> {
    let image = image::load_from_memory_with_format(bytes, ImageFormat::Png)?.into_rgba8();
    let (width, height) = image.dimensions();

    Ok(RgbaIcon {
        rgba: image.into_raw(),
        width,
        height,
    })
}

impl From<RgbaIcon> for IconData {
    fn from(icon: RgbaIcon) -> Self {
        Self {
            rgba: icon.rgba,
            width: icon.width,
            height: icon.height,
        }
    }
}
