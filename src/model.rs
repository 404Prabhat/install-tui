use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum BackendId {
    Pacman,
    Yay,
    Paru,
    Aura,
    Trizen,
}

impl BackendId {
    pub const ALL: [BackendId; 5] = [
        BackendId::Pacman,
        BackendId::Yay,
        BackendId::Paru,
        BackendId::Aura,
        BackendId::Trizen,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BackendId::Pacman => "pacman",
            BackendId::Yay => "yay",
            BackendId::Paru => "paru",
            BackendId::Aura => "aura",
            BackendId::Trizen => "trizen",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            BackendId::Pacman => "pacman",
            BackendId::Yay => "yay",
            BackendId::Paru => "paru",
            BackendId::Aura => "aura",
            BackendId::Trizen => "trizen",
        }
    }

    pub fn is_pacman(self) -> bool {
        matches!(self, BackendId::Pacman)
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display())
    }
}

#[derive(Clone, Debug)]
pub struct BackendState {
    pub id: BackendId,
    pub available: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Normal,
    Command,
    Filter,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Command => "COMMAND",
            Mode::Filter => "FILTER",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneFocus {
    List,
    Detail,
    Feed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortMode {
    Relevance,
    Name,
    Size,
    Repo,
    InstalledFirst,
}

impl SortMode {
    pub const ALL: [SortMode; 5] = [
        SortMode::Relevance,
        SortMode::Name,
        SortMode::Size,
        SortMode::Repo,
        SortMode::InstalledFirst,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortMode::Relevance => "relevance",
            SortMode::Name => "name",
            SortMode::Size => "size",
            SortMode::Repo => "repo",
            SortMode::InstalledFirst => "installed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueAction {
    Install,
    Remove,
}

impl QueueAction {
    pub fn label(self) -> &'static str {
        match self {
            QueueAction::Install => "install",
            QueueAction::Remove => "remove",
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub name: String,
    pub action: QueueAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RepoKind {
    Official,
    Aur,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageRecord {
    pub name: String,
    pub version: String,
    pub description: String,
    pub repo: String,
    pub size_bytes: i64,
    pub installed: bool,
    pub upgradable: bool,
    pub new_version: Option<String>,
    pub updated_at: i64,
    pub repo_kind: RepoKind,
}

impl PackageRecord {
    pub fn repo_badge(&self) -> &'static str {
        match self.repo_kind {
            RepoKind::Official => "repo",
            RepoKind::Aur => "AUR ⚠",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PackageDetail {
    pub package: String,
    pub rendered: String,
}

#[derive(Clone, Debug)]
pub struct InstallProgress {
    pub total: usize,
    pub done: usize,
    pub installed: usize,
    pub removed: usize,
    pub failed: usize,
    pub stage: String,
    pub paused: bool,
}

impl Default for InstallProgress {
    fn default() -> Self {
        Self {
            total: 0,
            done: 0,
            installed: 0,
            removed: 0,
            failed: 0,
            stage: "idle".to_string(),
            paused: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstallSummary {
    pub installed: usize,
    pub removed: usize,
    pub failed: usize,
    pub unresolved: Vec<String>,
    pub elapsed: Duration,
    pub log_path: PathBuf,
    pub aborted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedLevel {
    Info,
    Active,
    Success,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct FeedItem {
    pub when: DateTime<Local>,
    pub level: FeedLevel,
    pub message: String,
}

#[derive(Clone, Debug)]
pub enum SyncState {
    Idle { count: usize },
    Syncing,
    Error(String),
}

impl SyncState {
    pub fn badge(&self) -> String {
        match self {
            SyncState::Idle { count } => format!("✓ {} packages", count),
            SyncState::Syncing => "syncing…".to_string(),
            SyncState::Error(_) => "sync error".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PreviewData {
    pub package_count: usize,
    pub dependency_count: usize,
    pub total_download_bytes: i64,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum Overlay {
    None,
    Help,
    Preview(PreviewData),
    History,
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub file: PathBuf,
    pub modified: i64,
    pub title: String,
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    Feed { level: FeedLevel, message: String },
    SyncStarted,
    SyncFinished { count: usize, packages: Vec<PackageRecord> },
    SyncError(String),
    DetailLoaded(PackageDetail),
    DetailError { package: String, error: String },
    AurResults {
        query: String,
        packages: Vec<PackageRecord>,
    },
    InstallProgress(InstallProgress),
    InstallFinished(InstallSummary),
}

#[derive(Clone, Debug)]
pub struct KeybindItem {
    pub key: &'static str,
    pub action: &'static str,
}

pub fn keybinds() -> Vec<KeybindItem> {
    vec![
        KeybindItem {
            key: "j/k ↑/↓",
            action: "move list",
        },
        KeybindItem {
            key: "g / G",
            action: "top / bottom",
        },
        KeybindItem {
            key: "Enter",
            action: "toggle queue",
        },
        KeybindItem {
            key: "i",
            action: "install queue",
        },
        KeybindItem {
            key: "u / U",
            action: "upgradable view / full upgrade",
        },
        KeybindItem {
            key: "r",
            action: "toggle removal mode",
        },
        KeybindItem {
            key: "s",
            action: "cycle sort",
        },
        KeybindItem {
            key: "S",
            action: "sync database",
        },
        KeybindItem {
            key: "d / x",
            action: "drop queue item / clear queue",
        },
        KeybindItem {
            key: "t",
            action: "toggle dry-run",
        },
        KeybindItem {
            key: "Tab",
            action: "focus list/detail/feed",
        },
        KeybindItem {
            key: "1..5",
            action: "select backend index",
        },
        KeybindItem {
            key: "Alt+↑/↓",
            action: "reorder backend priority",
        },
        KeybindItem {
            key: "/",
            action: "filter mode",
        },
        KeybindItem {
            key: ":",
            action: "command mode",
        },
        KeybindItem {
            key: "?",
            action: "toggle help overlay",
        },
        KeybindItem {
            key: "q",
            action: "quit / graceful abort",
        },
    ]
}
