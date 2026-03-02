use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Normal,
    Filter,
    Command,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Filter => "FILTER",
            Self::Command => "COMMAND",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneFocus {
    List,
    Detail,
    Feed,
}

impl PaneFocus {
    pub fn next(self) -> Self {
        match self {
            Self::List => Self::Detail,
            Self::Detail => Self::Feed,
            Self::Feed => Self::List,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Detail => "detail",
            Self::Feed => "feed",
        }
    }
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
    pub fn next(self) -> Self {
        match self {
            Self::Relevance => Self::Name,
            Self::Name => Self::Size,
            Self::Size => Self::Repo,
            Self::Repo => Self::InstalledFirst,
            Self::InstalledFirst => Self::Relevance,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Name => "name",
            Self::Size => "size",
            Self::Repo => "repo",
            Self::InstalledFirst => "installed-first",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendId {
    Pacman,
    Yay,
    Paru,
    Aura,
    Trizen,
}

impl BackendId {
    pub fn all() -> [Self; 5] {
        [
            Self::Pacman,
            Self::Yay,
            Self::Paru,
            Self::Aura,
            Self::Trizen,
        ]
    }

    pub fn bin(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Yay => "yay",
            Self::Paru => "paru",
            Self::Aura => "aura",
            Self::Trizen => "trizen",
        }
    }

    pub fn label(self) -> &'static str {
        self.bin()
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pacman" => Some(Self::Pacman),
            "yay" => Some(Self::Yay),
            "paru" => Some(Self::Paru),
            "aura" => Some(Self::Aura),
            "trizen" => Some(Self::Trizen),
            _ => None,
        }
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Debug)]
pub struct BackendState {
    pub id: BackendId,
    pub available: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoKind {
    Official,
    Aur,
}

#[derive(Clone, Debug)]
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
    pub fn status_glyph(&self) -> &'static str {
        if self.upgradable {
            "↑"
        } else if self.installed {
            "●"
        } else {
            "○"
        }
    }

    pub fn size_human(&self) -> String {
        human_bytes(self.size_bytes)
    }

    pub fn repo_badge(&self) -> String {
        if self.repo_kind == RepoKind::Aur {
            "AUR ⚠".to_string()
        } else {
            self.repo.clone()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueAction {
    Install,
    Remove,
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub name: String,
    pub action: QueueAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedLevel {
    Resolve,
    Active,
    Done,
    Warning,
    Error,
    Info,
}

#[derive(Clone, Debug)]
pub struct FeedItem {
    pub ts: String,
    pub level: FeedLevel,
    pub text: String,
}

impl FeedItem {
    pub fn new(level: FeedLevel, text: impl Into<String>) -> Self {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        Self {
            ts,
            level,
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PackageDetail {
    pub package: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncState {
    Idle,
    Syncing,
    Ready(usize),
    Error,
}

#[derive(Clone, Debug)]
pub struct PreviewData {
    pub pkg_count: usize,
    pub dep_count: usize,
    pub total_download: String,
}

#[derive(Clone, Debug)]
pub enum Overlay {
    None,
    Help,
    ConfirmQuit,
    InstallPreview(PreviewData),
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    SyncStarted,
    SyncFinished {
        packages: Vec<PackageRecord>,
    },
    SyncError(String),
    DetailLoaded(PackageDetail),
    DetailError(String),
    AurResults {
        query: String,
        packages: Vec<PackageRecord>,
    },
    InstallLine(FeedItem),
    InstallFinished {
        installed: usize,
        skipped: usize,
        failed: usize,
        aborted: bool,
    },
}

#[derive(Clone, Debug)]
pub struct Keybind {
    pub key: &'static str,
    pub action: &'static str,
}

pub fn keybinds() -> Vec<Keybind> {
    vec![
        Keybind {
            key: "j / ↓",
            action: "Move down",
        },
        Keybind {
            key: "k / ↑",
            action: "Move up",
        },
        Keybind {
            key: "g / G",
            action: "Top / Bottom",
        },
        Keybind {
            key: "Enter",
            action: "Toggle queue item",
        },
        Keybind {
            key: "i",
            action: "Install queued items",
        },
        Keybind {
            key: "r",
            action: "Toggle removal mode",
        },
        Keybind {
            key: "s",
            action: "Cycle sort order",
        },
        Keybind {
            key: "S",
            action: "Foreground sync",
        },
        Keybind {
            key: "u",
            action: "Show upgrades",
        },
        Keybind {
            key: "U",
            action: "Full system upgrade",
        },
        Keybind {
            key: "d",
            action: "Remove highlighted from queue",
        },
        Keybind {
            key: "x",
            action: "Clear queue",
        },
        Keybind {
            key: "t",
            action: "Toggle dry-run",
        },
        Keybind {
            key: "/",
            action: "Filter mode",
        },
        Keybind {
            key: ":",
            action: "Command mode",
        },
        Keybind {
            key: "Tab",
            action: "Cycle focus pane",
        },
        Keybind {
            key: "q",
            action: "Quit",
        },
    ]
}

pub fn human_bytes(bytes: i64) -> String {
    if bytes <= 0 {
        return "-".to_string();
    }
    let value = bytes as f64;
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}
