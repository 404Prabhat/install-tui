use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::model::{InstallProgress, InstallSummary, Manager, ManagerAvailability};

const CHUNK_SIZE: usize = 120;

#[derive(Clone)]
pub struct InstallRequest {
    pub packages: Vec<String>,
    pub priority: [Manager; 3],
    pub availability: ManagerAvailability,
    pub official_set: HashSet<String>,
    pub dry_run: bool,
}

pub enum InstallEvent {
    Log(String),
    Progress(InstallProgress),
    Finished(InstallSummary),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PackageState {
    Pending,
    Installed,
    Skipped,
    Failed,
}

pub fn spawn_installer(request: InstallRequest, tx: Sender<InstallEvent>) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = Arc::clone(&cancel);

    thread::spawn(move || {
        if let Err(err) = run_installer(request, tx.clone(), thread_cancel) {
            let summary = InstallSummary {
                installed: 0,
                skipped: 0,
                failed: 1,
                unresolved: vec![err.to_string()],
                elapsed: Duration::from_secs(0),
                log_path: default_log_path(),
                aborted: false,
            };
            let _ = tx.send(InstallEvent::Log(format!("fatal: {err}")));
            let _ = tx.send(InstallEvent::Finished(summary));
        }
    });

    cancel
}

fn run_installer(
    request: InstallRequest,
    tx: Sender<InstallEvent>,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let started = Instant::now();
    let log_path = ensure_log_file()?;
    let mut log_file = OpenOptions::new()
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log file {}", log_path.display()))?;

    emit(
        &tx,
        &mut log_file,
        &format!(
            "start install packages={} priority={:?} dry_run={}",
            request.packages.len(),
            request.priority,
            request.dry_run
        ),
    );

    preflight(&tx, &mut log_file)?;

    if !request.dry_run {
        run_status("sudo", &["-v"])?;
        emit(&tx, &mut log_file, "sudo authentication cached");
    } else {
        emit(&tx, &mut log_file, "dry-run mode enabled");
    }

    let keepalive_flag = Arc::new(AtomicBool::new(false));
    let keepalive_thread = if request.dry_run {
        None
    } else {
        Some(start_sudo_keepalive(Arc::clone(&keepalive_flag)))
    };

    let queue = normalize_package_list(request.packages);
    let mut states: HashMap<String, PackageState> = HashMap::with_capacity(queue.len() * 2);
    for pkg in &queue {
        if package_installed(pkg) {
            states.insert(pkg.clone(), PackageState::Skipped);
            emit(
                &tx,
                &mut log_file,
                &format!("skip {pkg} (already installed)"),
            );
        } else {
            states.insert(pkg.clone(), PackageState::Pending);
        }
    }

    let mut progress = build_progress(&states, "Pre-check completed");
    progress.total = queue.len();
    let _ = tx.send(InstallEvent::Progress(progress.clone()));

    for manager in request.priority {
        if cancel.load(Ordering::Relaxed) {
            emit(
                &tx,
                &mut log_file,
                "abort requested; stopping further managers",
            );
            break;
        }

        if !request.availability.available(manager) {
            emit(
                &tx,
                &mut log_file,
                &format!("manager {} unavailable, skipping", manager.bin()),
            );
            continue;
        }

        let mut candidates = collect_pending(&states);
        if matches!(manager, Manager::Pacman) {
            candidates.retain(|pkg| request.official_set.contains(pkg));
        }

        if candidates.is_empty() {
            emit(
                &tx,
                &mut log_file,
                &format!("manager {} has no candidates", manager.bin()),
            );
            continue;
        }

        emit(
            &tx,
            &mut log_file,
            &format!(
                "stage {} candidates={} (batched)",
                manager.bin(),
                candidates.len()
            ),
        );

        progress.stage = format!("Installing via {}", manager.bin());
        let _ = tx.send(InstallEvent::Progress(progress.clone()));

        for chunk in candidates.chunks(CHUNK_SIZE) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let chunk_vec: Vec<String> = chunk.to_vec();

            if request.dry_run {
                for pkg in &chunk_vec {
                    if states.get(pkg) == Some(&PackageState::Pending) {
                        states.insert(pkg.clone(), PackageState::Installed);
                        emit(
                            &tx,
                            &mut log_file,
                            &format!("dry-run {} {pkg}", manager.bin()),
                        );
                    }
                }
                progress = build_progress(&states, &format!("Dry-run via {}", manager.bin()));
                progress.total = queue.len();
                let _ = tx.send(InstallEvent::Progress(progress.clone()));
                continue;
            }

            let failures = install_chunk_recursive(manager, &chunk_vec, &tx, &mut log_file)?;
            let failure_set: HashSet<String> = failures.into_iter().collect();

            for pkg in &chunk_vec {
                if package_installed(pkg) {
                    states.insert(pkg.clone(), PackageState::Installed);
                    continue;
                }

                if failure_set.contains(pkg) && states.get(pkg) == Some(&PackageState::Pending) {
                    emit(
                        &tx,
                        &mut log_file,
                        &format!("still unresolved after {}: {pkg}", manager.bin()),
                    );
                }
            }

            progress = build_progress(&states, &format!("Installed via {}", manager.bin()));
            progress.total = queue.len();
            let _ = tx.send(InstallEvent::Progress(progress.clone()));
        }
    }

    for pkg in collect_pending(&states) {
        states.insert(pkg.clone(), PackageState::Failed);
    }

    keepalive_flag.store(true, Ordering::Relaxed);
    if let Some(handle) = keepalive_thread {
        let _ = handle.join();
    }

    let unresolved = states
        .iter()
        .filter_map(|(pkg, state)| {
            if *state == PackageState::Failed {
                Some(pkg.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let summary = InstallSummary {
        installed: states
            .values()
            .filter(|state| **state == PackageState::Installed)
            .count(),
        skipped: states
            .values()
            .filter(|state| **state == PackageState::Skipped)
            .count(),
        failed: states
            .values()
            .filter(|state| **state == PackageState::Failed)
            .count(),
        unresolved,
        elapsed: started.elapsed(),
        log_path,
        aborted: cancel.load(Ordering::Relaxed),
    };

    let _ = tx.send(InstallEvent::Progress(InstallProgress {
        total: queue.len(),
        done: queue.len(),
        installed: summary.installed,
        skipped: summary.skipped,
        failed: summary.failed,
        stage: "Completed".to_string(),
    }));
    let _ = tx.send(InstallEvent::Finished(summary));

    Ok(())
}

fn install_chunk_recursive(
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
            &format!("batch ok {} count={}", manager.bin(), pkgs.len()),
        );
        return Ok(Vec::new());
    }

    if pkgs.len() == 1 {
        emit(
            tx,
            log_file,
            &format!("batch failed {} package={}", manager.bin(), pkgs[0]),
        );
        return Ok(vec![pkgs[0].clone()]);
    }

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
                    .map(|s| s.to_string()),
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
                .map(|s| s.to_string()),
            );
        }
        Manager::Paru => {
            program = "paru";
            args.extend(
                ["-S", "--noconfirm", "--needed", "--skipreview"]
                    .iter()
                    .map(|s| s.to_string()),
            );
        }
    }

    args.extend(pkgs.iter().cloned());
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(run_status(program, &arg_refs).is_ok())
}

fn normalize_package_list(input: Vec<String>) -> Vec<String> {
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

fn collect_pending(states: &HashMap<String, PackageState>) -> Vec<String> {
    states
        .iter()
        .filter_map(|(pkg, state)| {
            if *state == PackageState::Pending {
                Some(pkg.clone())
            } else {
                None
            }
        })
        .collect()
}

fn build_progress(states: &HashMap<String, PackageState>, stage: &str) -> InstallProgress {
    InstallProgress {
        total: states.len(),
        done: states
            .values()
            .filter(|state| **state != PackageState::Pending)
            .count(),
        installed: states
            .values()
            .filter(|state| **state == PackageState::Installed)
            .count(),
        skipped: states
            .values()
            .filter(|state| **state == PackageState::Skipped)
            .count(),
        failed: states
            .values()
            .filter(|state| **state == PackageState::Failed)
            .count(),
        stage: stage.to_string(),
    }
}

fn preflight(tx: &Sender<InstallEvent>, log_file: &mut File) -> Result<()> {
    emit(tx, log_file, "preflight checking arch + user + internet");

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
        .map(|s| s.success())
        .unwrap_or(false);

    if !ping_ok {
        anyhow::bail!("internet check failed (cannot ping archlinux.org)");
    }

    Ok(())
}

fn package_installed(pkg: &str) -> bool {
    Command::new("pacman")
        .args(["-Q", pkg])
        .status()
        .map(|s| s.success())
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

fn emit(tx: &Sender<InstallEvent>, log_file: &mut File, message: &str) {
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

fn ensure_log_file() -> Result<PathBuf> {
    let path = default_log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let _ = File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(path)
}

fn default_log_path() -> PathBuf {
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

fn start_sudo_keepalive(stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
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
