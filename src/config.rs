use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::BackendId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub backend: BackendConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub keybinds: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BackendConfig {
    #[serde(default = "default_backend_priority")]
    pub priority: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_true")]
    pub show_sizes: bool,
    #[serde(default = "default_date_format")]
    pub date_format: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BehaviorConfig {
    #[serde(default = "default_true")]
    pub confirm_on_quit: bool,
    #[serde(default = "default_true")]
    pub auto_sync_on_start: bool,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_hours: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavedSets {
    #[serde(default)]
    pub sets: BTreeMap<String, Vec<String>>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            backend: BackendConfig {
                priority: default_backend_priority(),
            },
            ui: UiConfig {
                theme: default_theme(),
                show_sizes: default_true(),
                date_format: default_date_format(),
            },
            behavior: BehaviorConfig {
                confirm_on_quit: default_true(),
                auto_sync_on_start: default_true(),
                cache_ttl_hours: default_cache_ttl(),
            },
            keybinds: BTreeMap::new(),
        }
    }
}

fn default_backend_priority() -> Vec<String> {
    vec!["pacman".to_string(), "yay".to_string(), "paru".to_string()]
}

fn default_theme() -> String {
    "arch-dark".to_string()
}

fn default_true() -> bool {
    true
}

fn default_cache_ttl() -> u64 {
    4
}

fn default_date_format() -> String {
    "%H:%M:%S".to_string()
}

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config")
        .join("arch-package-tui")
}

pub fn cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".cache")
        .join("arch-package-tui")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn sets_path() -> PathBuf {
    config_dir().join("sets.toml")
}

pub fn load_or_create_config() -> Result<AppConfig> {
    fs::create_dir_all(config_dir()).context("failed to create config directory")?;
    let path = config_path();
    if !path.exists() {
        let default = AppConfig::default();
        save_config(&default)?;
        return Ok(default);
    }

    let data = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    let cfg: AppConfig = toml::from_str(&data)
        .with_context(|| format!("failed to parse config {}", path.display()))?;
    Ok(cfg)
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    fs::create_dir_all(config_dir()).context("failed to create config directory")?;
    let text = toml::to_string_pretty(config).context("failed to serialize config")?;
    fs::write(config_path(), text).context("failed to write config")?;
    Ok(())
}

pub fn load_sets() -> Result<SavedSets> {
    fs::create_dir_all(config_dir()).context("failed to create config directory")?;
    let path = sets_path();
    if !path.exists() {
        let sets = SavedSets::default();
        save_sets(&sets)?;
        return Ok(sets);
    }

    let data = fs::read_to_string(&path)
        .with_context(|| format!("failed to read sets {}", path.display()))?;
    let sets: SavedSets = toml::from_str(&data)
        .with_context(|| format!("failed to parse sets {}", path.display()))?;
    Ok(sets)
}

pub fn save_sets(sets: &SavedSets) -> Result<()> {
    fs::create_dir_all(config_dir()).context("failed to create config directory")?;
    let text = toml::to_string_pretty(sets).context("failed to serialize sets")?;
    fs::write(sets_path(), text).context("failed to write sets")?;
    Ok(())
}

pub fn parse_backend_priority(priority: &[String]) -> Vec<BackendId> {
    let mut out = Vec::new();
    for item in priority {
        if let Some(id) = parse_backend_id(item)
            && !out.contains(&id)
        {
            out.push(id);
        }
    }

    for id in BackendId::all() {
        if !out.contains(&id) {
            out.push(id);
        }
    }

    out
}

pub fn parse_backend_id(value: &str) -> Option<BackendId> {
    BackendId::from_str(&value.to_lowercase())
}
