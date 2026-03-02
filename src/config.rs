use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::BackendId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub backend: BackendSection,
    pub ui: UiSection,
    pub behavior: BehaviorSection,
    pub keybinds: BTreeMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            backend: BackendSection::default(),
            ui: UiSection::default(),
            behavior: BehaviorSection::default(),
            keybinds: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BackendSection {
    pub priority: Vec<String>,
}

impl Default for BackendSection {
    fn default() -> Self {
        Self {
            priority: vec!["pacman".to_string(), "yay".to_string(), "paru".to_string()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSection {
    pub theme: String,
    pub show_sizes: bool,
    pub date_format: String,
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            theme: "arch-dark".to_string(),
            show_sizes: true,
            date_format: "%H:%M:%S".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorSection {
    pub confirm_on_quit: bool,
    pub auto_sync_on_start: bool,
    pub cache_ttl_hours: i64,
}

impl Default for BehaviorSection {
    fn default() -> Self {
        Self {
            confirm_on_quit: true,
            auto_sync_on_start: true,
            cache_ttl_hours: 4,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedSets {
    pub sets: BTreeMap<String, Vec<String>>,
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("arch-package-tui")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("arch-package-tui")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn sets_path() -> PathBuf {
    config_dir().join("sets.toml")
}

pub fn history_dir() -> PathBuf {
    cache_dir()
}

pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(config_dir()).context("failed to create config dir")?;
    fs::create_dir_all(cache_dir()).context("failed to create cache dir")?;
    Ok(())
}

pub fn load_or_create_config() -> Result<AppConfig> {
    ensure_dirs()?;
    let path = config_path();

    if !path.exists() {
        let default = AppConfig::default();
        save_config(&default)?;
        return Ok(default);
    }

    let raw = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: AppConfig = toml::from_str(&raw).context("failed to parse config.toml")?;
    Ok(parsed)
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    ensure_dirs()?;
    let path = config_path();
    let raw = toml::to_string_pretty(config).context("failed to serialize config")?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
}

pub fn load_sets() -> Result<SavedSets> {
    ensure_dirs()?;
    let path = sets_path();
    if !path.exists() {
        let sets = SavedSets::default();
        save_sets(&sets)?;
        return Ok(sets);
    }

    let raw = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let sets = toml::from_str::<SavedSets>(&raw).context("failed to parse sets.toml")?;
    Ok(sets)
}

pub fn save_sets(sets: &SavedSets) -> Result<()> {
    ensure_dirs()?;
    let path = sets_path();
    let raw = toml::to_string_pretty(sets).context("failed to serialize sets.toml")?;
    fs::write(&path, raw).with_context(|| format!("failed to write {}", path.display()))
}

pub fn parse_backend_priority(config: &AppConfig) -> Vec<BackendId> {
    let mut out = Vec::new();
    for raw in &config.backend.priority {
        if let Some(id) = parse_backend_id(raw) {
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }

    for id in BackendId::ALL {
        if !out.contains(&id) {
            out.push(id);
        }
    }

    out
}

pub fn parse_backend_id(raw: &str) -> Option<BackendId> {
    match raw.trim().to_lowercase().as_str() {
        "pacman" => Some(BackendId::Pacman),
        "yay" => Some(BackendId::Yay),
        "paru" => Some(BackendId::Paru),
        "aura" => Some(BackendId::Aura),
        "trizen" => Some(BackendId::Trizen),
        _ => None,
    }
}
