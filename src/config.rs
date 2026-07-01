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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub movement_mode: MovementMode,
    #[serde(default)]
    pub start_binding: Option<Binding>,
    #[serde(default)]
    pub stop_binding: Option<Binding>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            movement_mode: MovementMode::Realistic,
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
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
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
