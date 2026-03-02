use std::collections::HashMap;
use std::process::Command;

use crate::model::{BackendId, BackendState};

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn as_debug_string(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

#[allow(dead_code)]
pub trait PackageBackend: Send + Sync {
    fn id(&self) -> BackendId;
    fn install_cmd(&self, packages: &[String]) -> CommandSpec;
    fn remove_cmd(&self, packages: &[String]) -> CommandSpec;
    fn sync_cmd(&self) -> CommandSpec;
    fn full_upgrade_cmd(&self) -> CommandSpec;
    fn info_cmd(&self, package: &str) -> CommandSpec;
}

#[derive(Clone, Debug)]
pub struct GenericBackend {
    id: BackendId,
}

impl GenericBackend {
    pub fn new(id: BackendId) -> Self {
        Self { id }
    }
}

impl PackageBackend for GenericBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn install_cmd(&self, packages: &[String]) -> CommandSpec {
        match self.id {
            BackendId::Pacman => {
                let mut args = vec![
                    "pacman".to_string(),
                    "-S".to_string(),
                    "--noconfirm".to_string(),
                    "--needed".to_string(),
                ];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "sudo".to_string(),
                    args,
                }
            }
            BackendId::Yay => {
                let mut args = vec![
                    "-S".to_string(),
                    "--noconfirm".to_string(),
                    "--needed".to_string(),
                    "--answerclean".to_string(),
                    "None".to_string(),
                    "--answerdiff".to_string(),
                    "None".to_string(),
                ];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "yay".to_string(),
                    args,
                }
            }
            BackendId::Paru => {
                let mut args = vec![
                    "-S".to_string(),
                    "--noconfirm".to_string(),
                    "--needed".to_string(),
                    "--skipreview".to_string(),
                ];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "paru".to_string(),
                    args,
                }
            }
            BackendId::Aura => {
                let mut args = vec!["-S".to_string(), "--noconfirm".to_string()];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "aura".to_string(),
                    args,
                }
            }
            BackendId::Trizen => {
                let mut args = vec![
                    "-S".to_string(),
                    "--noconfirm".to_string(),
                    "--needed".to_string(),
                ];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "trizen".to_string(),
                    args,
                }
            }
        }
    }

    fn remove_cmd(&self, packages: &[String]) -> CommandSpec {
        match self.id {
            BackendId::Pacman => {
                let mut args = vec![
                    "pacman".to_string(),
                    "-Rns".to_string(),
                    "--noconfirm".to_string(),
                ];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "sudo".to_string(),
                    args,
                }
            }
            BackendId::Yay => {
                let mut args = vec!["-Rns".to_string(), "--noconfirm".to_string()];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "yay".to_string(),
                    args,
                }
            }
            BackendId::Paru => {
                let mut args = vec!["-Rns".to_string(), "--noconfirm".to_string()];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "paru".to_string(),
                    args,
                }
            }
            BackendId::Aura => {
                let mut args = vec!["-R".to_string(), "--noconfirm".to_string()];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "aura".to_string(),
                    args,
                }
            }
            BackendId::Trizen => {
                let mut args = vec!["-Rns".to_string(), "--noconfirm".to_string()];
                args.extend(packages.iter().cloned());
                CommandSpec {
                    program: "trizen".to_string(),
                    args,
                }
            }
        }
    }

    fn sync_cmd(&self) -> CommandSpec {
        match self.id {
            BackendId::Pacman => CommandSpec {
                program: "sudo".to_string(),
                args: vec!["pacman".to_string(), "-Sy".to_string()],
            },
            _ => CommandSpec {
                program: self.id.bin().to_string(),
                args: vec!["-Sy".to_string()],
            },
        }
    }

    fn full_upgrade_cmd(&self) -> CommandSpec {
        match self.id {
            BackendId::Pacman => CommandSpec {
                program: "sudo".to_string(),
                args: vec![
                    "pacman".to_string(),
                    "-Syu".to_string(),
                    "--noconfirm".to_string(),
                ],
            },
            _ => CommandSpec {
                program: self.id.bin().to_string(),
                args: vec!["-Syu".to_string(), "--noconfirm".to_string()],
            },
        }
    }

    fn info_cmd(&self, package: &str) -> CommandSpec {
        match self.id {
            BackendId::Pacman => CommandSpec {
                program: "pacman".to_string(),
                args: vec!["-Si".to_string(), package.to_string()],
            },
            BackendId::Aura => CommandSpec {
                program: "aura".to_string(),
                args: vec!["-Ai".to_string(), package.to_string()],
            },
            _ => CommandSpec {
                program: self.id.bin().to_string(),
                args: vec!["-Si".to_string(), package.to_string()],
            },
        }
    }
}

pub fn create_backends() -> HashMap<BackendId, Box<dyn PackageBackend>> {
    let mut map: HashMap<BackendId, Box<dyn PackageBackend>> = HashMap::new();
    for id in BackendId::all() {
        map.insert(id, Box::new(GenericBackend::new(id)));
    }
    map
}

pub fn detect_backend_states() -> Vec<BackendState> {
    BackendId::all()
        .into_iter()
        .map(|id| BackendState {
            id,
            available: command_exists(id.bin()),
        })
        .collect()
}

pub fn command_exists(bin: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {bin} >/dev/null")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
