mod ops;

use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::model::{InstallProgress, InstallSummary, Manager, ManagerAvailability};

use self::ops::{
    build_progress, collect_pending, emit, ensure_log_file, install_chunk_recursive,
    normalize_package_list, package_installed, preflight, start_sudo_keepalive,
};

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
                log_path: ops::default_log_path(),
                aborted: false,
            };
            let _ = tx.send(InstallEvent::Log(format!("error: fatal installer failure: {err}")));
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
    let mut log_file = OpenOptions::new().append(true).open(&log_path)?;

    emit(
        &tx,
        &mut log_file,
        &format!(
            "info: start install packages={} priority={:?} dry_run={}",
            request.packages.len(),
            request.priority,
            request.dry_run
        ),
    );

    preflight(&tx, &mut log_file)?;

    if request.dry_run {
        emit(&tx, &mut log_file, "warn: dry-run mode enabled");
    } else {
        emit(
            &tx,
            &mut log_file,
            "info: using existing sudo session (validated by UI pre-check)",
        );
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
                &format!("warn: skip {pkg} (already installed)"),
            );
        } else {
            states.insert(pkg.clone(), PackageState::Pending);
        }
    }

    let mut progress = build_progress(&states, "Pre-check completed", None);
    progress.total = queue.len();
    let _ = tx.send(InstallEvent::Progress(progress.clone()));

    for manager in request.priority {
        if cancel.load(Ordering::Relaxed) {
            emit(
                &tx,
                &mut log_file,
                "warn: abort requested; stopping further managers",
            );
            break;
        }

        if !request.availability.available(manager) {
            emit(
                &tx,
                &mut log_file,
                &format!("warn: manager {} unavailable, skipping", manager.bin()),
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
                &format!("info: manager {} has no candidates", manager.bin()),
            );
            continue;
        }

        emit(
            &tx,
            &mut log_file,
            &format!(
                "info: stage {} candidates={} (batched)",
                manager.bin(),
                candidates.len()
            ),
        );

        for chunk in candidates.chunks(CHUNK_SIZE) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let chunk_vec: Vec<String> = chunk.to_vec();
            let chunk_head = chunk_vec.first().cloned();
            progress = build_progress(
                &states,
                &format!("Installing via {}", manager.bin()),
                chunk_head,
            );
            progress.total = queue.len();
            let _ = tx.send(InstallEvent::Progress(progress.clone()));

            if request.dry_run {
                for pkg in &chunk_vec {
                    if states.get(pkg) == Some(&PackageState::Pending) {
                        states.insert(pkg.clone(), PackageState::Installed);
                        emit(&tx, &mut log_file, &format!("ok: dry-run {} {pkg}", manager.bin()));
                    }
                }

                progress = build_progress(
                    &states,
                    &format!("Dry-run via {}", manager.bin()),
                    chunk_vec.last().cloned(),
                );
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
                        &format!("error: unresolved after {}: {pkg}", manager.bin()),
                    );
                }
            }

            progress = build_progress(
                &states,
                &format!("Installed via {}", manager.bin()),
                chunk_vec.last().cloned(),
            );
            progress.total = queue.len();
            let _ = tx.send(InstallEvent::Progress(progress.clone()));
        }
    }

    for pkg in collect_pending(&states) {
        states.insert(pkg, PackageState::Failed);
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
        current_package: None,
    }));
    let _ = tx.send(InstallEvent::Finished(summary));

    Ok(())
}
