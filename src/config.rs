use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementMode {
    #[default]
    Realistic,
    Simple,
}

impl MovementMode {
    pub const ALL: [Self; 2] = [Self::Realistic, Self::Simple];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Realistic => "Realistic",
            Self::Simple => "Simple",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppTheme {
    #[default]
    System,
    Light,
    Dark,
    OledBlack,
    GentleGecko,
    BlackKnight,
    Diamond,
    Dreams,
    Paranoid,
    RedVelvet,
    Subspace,
    Tiefling,
    Vibes,
}

impl AppTheme {
    pub const ALL: [Self; 13] = [
        Self::System,
        Self::Light,
        Self::Dark,
        Self::OledBlack,
        Self::GentleGecko,
        Self::BlackKnight,
        Self::Diamond,
        Self::Dreams,
        Self::Paranoid,
        Self::RedVelvet,
        Self::Subspace,
        Self::Tiefling,
        Self::Vibes,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "Follow system",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::OledBlack => "OLED Black",
            Self::GentleGecko => "Gentle Gecko",
            Self::BlackKnight => "Black Knight",
            Self::Diamond => "Diamond",
            Self::Dreams => "Dreams",
            Self::Paranoid => "Paranoid",
            Self::RedVelvet => "Red Velvet",
            Self::Subspace => "Subspace",
            Self::Tiefling => "Tiefling",
            Self::Vibes => "Vibes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    Key,
    MouseButton,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub kind: BindingKind,
    pub code: String,
    pub label: String,
}

impl Binding {
    pub fn display_label(&self) -> &str {
        &self.label
    }

    pub fn is_left_click(&self) -> bool {
        self.kind == BindingKind::MouseButton && self.label.rsplit('+').next() == Some("Mouse Left")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub movement_mode: MovementMode,
    #[serde(default)]
    pub theme: AppTheme,
    #[serde(default)]
    pub start_binding: Option<Binding>,
    #[serde(default)]
    pub stop_binding: Option<Binding>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            movement_mode: MovementMode::Realistic,
            theme: AppTheme::System,
            start_binding: None,
            stop_binding: None,
        }
    }
}

pub fn config_path() -> PathBuf {
    ProjectDirs::from("com", "Visorcraft", "Realistic Mouse Jiggler")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("realistic-mouse-jiggler.toml"))
}

pub fn load_config(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let mut config: AppConfig = toml::from_str(&contents).unwrap_or_default();
            config.start_binding = config
                .start_binding
                .filter(|binding| !binding.is_left_click());
            config.stop_binding = config
                .stop_binding
                .filter(|binding| !binding.is_left_click());
            config
        }
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(path: &Path, config: &AppConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(config)?;
    fs::write(path, contents)?;
    Ok(())
}
