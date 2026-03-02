use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

use crate::model::{BackendId, BackendState};

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn commandline(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

pub trait PackageBackend: Send + Sync {
    fn id(&self) -> BackendId;
    fn available(&self) -> bool;
    fn install_spec(&self, packages: &[String]) -> CommandSpec;
    fn remove_spec(&self, packages: &[String]) -> CommandSpec;
    fn sync_spec(&self) -> CommandSpec;
    fn full_upgrade_spec(&self) -> CommandSpec;
    fn info_spec(&self, package: &str) -> CommandSpec;
}

#[derive(Clone)]
struct GenericBackend {
    id: BackendId,
    available: bool,
}

impl GenericBackend {
    fn new(id: BackendId) -> Self {
        let available = command_exists(id.as_str());
        Self { id, available }
    }

    fn base_bin(&self) -> &'static str {
        self.id.as_str()
    }

    fn use_sudo(&self) -> bool {
        self.id.is_pacman()
    }

    fn wrap_sudo(&self, mut args: Vec<String>) -> CommandSpec {
        if self.use_sudo() {
            let mut wrapped = Vec::with_capacity(args.len() + 1);
            wrapped.push(self.base_bin().to_string());
            wrapped.append(&mut args);
            CommandSpec {
                program: "sudo".to_string(),
                args: wrapped,
            }
        } else {
            CommandSpec {
                program: self.base_bin().to_string(),
                args,
            }
        }
    }
}

impl PackageBackend for GenericBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn available(&self) -> bool {
        self.available
    }

    fn install_spec(&self, packages: &[String]) -> CommandSpec {
        let mut args = vec![
            "-S".to_string(),
            "--noconfirm".to_string(),
            "--needed".to_string(),
        ];

        if matches!(self.id, BackendId::Paru) {
            args.push("--skipreview".to_string());
        }

        if matches!(self.id, BackendId::Yay | BackendId::Aura | BackendId::Trizen) {
            args.push("--answerclean".to_string());
            args.push("None".to_string());
            args.push("--answerdiff".to_string());
            args.push("None".to_string());
        }

        args.extend(packages.iter().cloned());
        self.wrap_sudo(args)
    }

    fn remove_spec(&self, packages: &[String]) -> CommandSpec {
        let mut args = vec![
            "-Rns".to_string(),
            "--noconfirm".to_string(),
            "--needed".to_string(),
        ];
        args.extend(packages.iter().cloned());
        self.wrap_sudo(args)
    }

    fn sync_spec(&self) -> CommandSpec {
        let args = vec!["-Sy".to_string()];
        self.wrap_sudo(args)
    }

    fn full_upgrade_spec(&self) -> CommandSpec {
        let args = vec!["-Syu".to_string(), "--noconfirm".to_string()];
        self.wrap_sudo(args)
    }

    fn info_spec(&self, package: &str) -> CommandSpec {
        CommandSpec {
            program: self.base_bin().to_string(),
            args: vec!["-Si".to_string(), package.to_string()],
        }
    }
}

pub fn create_backends() -> (HashMap<BackendId, Arc<dyn PackageBackend>>, Vec<BackendState>) {
    let mut map: HashMap<BackendId, Arc<dyn PackageBackend>> = HashMap::new();
    let mut states = Vec::new();

    for id in BackendId::ALL {
        let backend = GenericBackend::new(id);
        states.push(BackendState {
            id,
            available: backend.available(),
        });
        map.insert(id, Arc::new(backend));
    }

    (map, states)
}

pub fn command_exists(bin: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {bin} >/dev/null")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
