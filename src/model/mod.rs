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
            Manager::Pacman => "pacman",
            Manager::Yay => "yay",
            Manager::Paru => "paru",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Manager::Pacman => "PACMAN",
            Manager::Yay => "YAY",
            Manager::Paru => "PARU",
        }
    }
}

impl fmt::Display for Manager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.bin())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ManagerAvailability {
    pub pacman: bool,
    pub yay: bool,
    pub paru: bool,
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
            "pacman={} yay={} paru={}",
            yes_no(self.pacman),
            yes_no(self.yay),
            yes_no(self.paru)
        )
    }

    pub fn aur_helper(&self) -> Option<Manager> {
        if self.yay {
            Some(Manager::Yay)
        } else if self.paru {
            Some(Manager::Paru)
        } else {
            None
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

pub const PRIORITY_PRESETS: [[Manager; 3]; 4] = [
    [Manager::Pacman, Manager::Yay, Manager::Paru],
    [Manager::Pacman, Manager::Paru, Manager::Yay],
    [Manager::Yay, Manager::Paru, Manager::Pacman],
    [Manager::Paru, Manager::Yay, Manager::Pacman],
];

pub fn priority_to_text(priority: &[Manager; 3]) -> String {
    format!(
        "{} -> {} -> {}",
        priority[0].bin(),
        priority[1].bin(),
        priority[2].bin()
    )
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
            RepoHint::Official => "official",
            RepoHint::Aur => "aur",
            RepoHint::Both => "official+aur",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PackageRecord {
    pub lower: String,
    pub name: String,
    pub repo: RepoHint,
}

#[derive(Clone, Debug, Default)]
pub struct InstallProgress {
    pub total: usize,
    pub done: usize,
    pub installed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub stage: String,
    pub current_package: Option<String>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
}
