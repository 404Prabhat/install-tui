use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Manager {
    Pacman,
    Yay,
    Paru,
}

impl Manager {
    pub fn bin(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Yay => "yay",
            Self::Paru => "paru",
        }
    }
}

impl fmt::Display for Manager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.bin())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoHint {
    Official,
    Aur,
    Both,
}

impl RepoHint {
    pub fn label(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Aur => "aur",
            Self::Both => "official+aur",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PackageRecord {
    pub name: String,
    pub lower: String,
    pub repo: RepoHint,
}

#[derive(Clone, Debug)]
pub struct ManagerAvailability {
    pub pacman: bool,
    pub yay: bool,
    pub paru: bool,
}

impl Default for ManagerAvailability {
    fn default() -> Self {
        Self {
            pacman: true,
            yay: false,
            paru: false,
        }
    }
}

impl ManagerAvailability {
    pub fn available(&self, manager: Manager) -> bool {
        match manager {
            Manager::Pacman => self.pacman,
            Manager::Yay => self.yay,
            Manager::Paru => self.paru,
        }
    }

    pub fn line(&self) -> String {
        format!(
            "pacman={}  yay={}  paru={}",
            bool_tag(self.pacman),
            bool_tag(self.yay),
            bool_tag(self.paru)
        )
    }
}

fn bool_tag(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

pub const PRIORITY_PRESETS: [[Manager; 3]; 6] = [
    [Manager::Pacman, Manager::Yay, Manager::Paru],
    [Manager::Pacman, Manager::Paru, Manager::Yay],
    [Manager::Yay, Manager::Pacman, Manager::Paru],
    [Manager::Yay, Manager::Paru, Manager::Pacman],
    [Manager::Paru, Manager::Pacman, Manager::Yay],
    [Manager::Paru, Manager::Yay, Manager::Pacman],
];

pub fn priority_to_text(order: &[Manager; 3]) -> String {
    format!("{} -> {} -> {}", order[0], order[1], order[2])
}

#[derive(Clone, Debug, Default)]
pub struct InstallProgress {
    pub total: usize,
    pub done: usize,
    pub installed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub stage: String,
}

#[derive(Clone, Debug)]
pub struct InstallSummary {
    pub installed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub unresolved: Vec<String>,
    pub elapsed: Duration,
    pub log_path: PathBuf,
    pub aborted: bool,
}
