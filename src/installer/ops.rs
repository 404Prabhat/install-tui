use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::model::Manager;
use crate::model::InstallProgress;

use super::InstallEvent;

pub(super) fn install_chunk_recursive(
    manager: Manager,
    pkgs: &[String],
    tx: &Sender<InstallEvent>,
    log_file: &mut File,
) -> Result<Vec<String>> {
    if pkgs.is_empty() {
        return Ok(Vec::new());
    }

    if run_install_batch(manager, pkgs)? {
        emit(
            tx,
            log_file,
            &format!("ok: batch ok {} count={}", manager.bin(), pkgs.len()),
        );
        return Ok(Vec::new());
    }

    if pkgs.len() == 1 {
        emit(
            tx,
            log_file,
            &format!("error: batch failed {} package={}", manager.bin(), pkgs[0]),
        );
        return Ok(vec![pkgs[0].clone()]);
    }

    // Binary splitting isolates problematic packages instead of failing the whole queue.
    let mid = pkgs.len() / 2;
    let left = install_chunk_recursive(manager, &pkgs[..mid], tx, log_file)?;
    let right = install_chunk_recursive(manager, &pkgs[mid..], tx, log_file)?;

    let mut failed = left;
    failed.extend(right);
    Ok(failed)
}

fn run_install_batch(manager: Manager, pkgs: &[String]) -> Result<bool> {
    let mut args: Vec<String> = Vec::new();
    let program: &str;

    match manager {
        Manager::Pacman => {
            program = "sudo";
            args.extend(
                ["pacman", "-S", "--noconfirm", "--needed"]
                    .iter()
                    .map(|value| value.to_string()),
            );
        }
        Manager::Yay => {
            program = "yay";
            args.extend(
                [
                    "-S",
                    "--noconfirm",
                    "--needed",
                    "--answerclean",
                    "None",
                    "--answerdiff",
                    "None",
                ]
                .iter()
                .map(|value| value.to_string()),
            );
        }
        Manager::Paru => {
            program = "paru";
            args.extend(
                ["-S", "--noconfirm", "--needed", "--skipreview"]
                    .iter()
                    .map(|value| value.to_string()),
            );
        }
    }

    args.extend(pkgs.iter().cloned());
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(run_status(program, &arg_refs).is_ok())
}

pub(super) fn normalize_package_list(input: Vec<String>) -> Vec<String> {
    let mut dedup = HashSet::new();
    let mut out = Vec::new();

    for pkg in input {
        let trimmed = pkg.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cleaned = trimmed
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '+' | '@'))
            .collect::<String>();

        if cleaned.is_empty() {
            continue;
        }

        if dedup.insert(cleaned.clone()) {
            out.push(cleaned);
        }
    }

    out
}

pub(super) fn collect_pending(states: &HashMap<String, super::PackageState>) -> Vec<String> {
    states
        .iter()
        .filter_map(|(pkg, state)| {
            if *state == super::PackageState::Pending {
                Some(pkg.clone())
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn build_progress(
    states: &HashMap<String, super::PackageState>,
    stage: &str,
    current_package: Option<String>,
) -> InstallProgress {
    InstallProgress {
        total: states.len(),
        done: states
            .values()
            .filter(|state| **state != super::PackageState::Pending)
            .count(),
        installed: states
            .values()
            .filter(|state| **state == super::PackageState::Installed)
            .count(),
        skipped: states
            .values()
            .filter(|state| **state == super::PackageState::Skipped)
            .count(),
        failed: states
            .values()
            .filter(|state| **state == super::PackageState::Failed)
            .count(),
        stage: stage.to_string(),
        current_package,
    }
}

pub(super) fn preflight(tx: &Sender<InstallEvent>, log_file: &mut File) -> Result<()> {
    emit(tx, log_file, "info: preflight checking arch + user + internet");

    if !std::path::Path::new("/etc/arch-release").exists() {
        anyhow::bail!("this installer is for Arch Linux only");
    }

    let uid_output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to run id -u")?;
    let uid = String::from_utf8_lossy(&uid_output.stdout)
        .trim()
        .to_string();
    if uid == "0" {
        anyhow::bail!("run as non-root user");
    }

    let ping_ok = Command::new("ping")
        .args(["-c", "1", "-W", "3", "archlinux.org"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if !ping_ok {
        anyhow::bail!("internet check failed (cannot ping archlinux.org)");
    }

    Ok(())
}

pub(super) fn package_installed(pkg: &str) -> bool {
    Command::new("pacman")
        .args(["-Q", pkg])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_status(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute: {} {}", program, args.join(" ")))?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("non-zero exit: {} {}", program, args.join(" "))
    }
}

pub(super) fn emit(tx: &Sender<InstallEvent>, log_file: &mut File, message: &str) {
    let line = format!("[{}] {}", now_hms(), message);
    let _ = writeln!(log_file, "{line}");
    let _ = log_file.flush();
    let _ = tx.send(InstallEvent::Log(line));
}

fn now_hms() -> String {
    let out = Command::new("date").args(["+%H:%M:%S"]).output();
    match out {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "00:00:00".to_string(),
    }
}

pub(super) fn ensure_log_file() -> Result<PathBuf> {
    let path = default_log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let _ = File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(path)
}

pub(super) fn default_log_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    PathBuf::from(home)
        .join(".cache")
        .join("arch-package-tui")
        .join(format!("install-{stamp}.log"))
}

pub(super) fn start_sudo_keepalive(stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let _ = Command::new("sudo").args(["-n", "true"]).status();
            for _ in 0..240 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_secs(1));
            }
        }
    })
}
