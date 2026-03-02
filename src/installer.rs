use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc::UnboundedSender;

use crate::backend::{PackageBackend, create_backends};
use crate::model::{AppEvent, BackendId, FeedItem, FeedLevel, QueueAction, QueueItem};

#[derive(Clone)]
pub struct InstallHandle {
    pub cancel: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
}

pub fn spawn_install(
    queue: Vec<QueueItem>,
    backend_priority: Vec<BackendId>,
    backend_available: HashMap<BackendId, bool>,
    dry_run: bool,
    tx: UnboundedSender<AppEvent>,
) -> InstallHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(AtomicBool::new(false));

    let task_cancel = Arc::clone(&cancel);
    let task_pause = Arc::clone(&pause);

    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            run_install(
                queue,
                backend_priority,
                backend_available,
                dry_run,
                tx,
                task_cancel,
                task_pause,
            )
        })
        .await;

        if let Err(err) = result {
            eprintln!("install task join error: {err}");
        }
    });

    InstallHandle { cancel, pause }
}

pub fn spawn_full_upgrade(
    backend_priority: Vec<BackendId>,
    backend_available: HashMap<BackendId, bool>,
    dry_run: bool,
    tx: UnboundedSender<AppEvent>,
) {
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            let backends = create_backends();
            if let Err(err) = preflight() {
                let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                    FeedLevel::Error,
                    format!("preflight failed: {err}"),
                )));
                let _ = tx.send(AppEvent::InstallFinished {
                    installed: 0,
                    skipped: 0,
                    failed: 1,
                    aborted: false,
                });
                return;
            }

            for backend in backend_priority {
                if !backend_available.get(&backend).copied().unwrap_or(false) {
                    continue;
                }

                let Some(handler) = backends.get(&backend) else {
                    continue;
                };

                let cmd = handler.full_upgrade_cmd();
                if dry_run {
                    let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                        FeedLevel::Info,
                        format!("dry-run: {}", cmd.as_debug_string()),
                    )));
                    let _ = tx.send(AppEvent::InstallFinished {
                        installed: 0,
                        skipped: 0,
                        failed: 0,
                        aborted: false,
                    });
                    return;
                }

                let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                    FeedLevel::Active,
                    format!("running full upgrade via {}", backend.bin()),
                )));

                match run_command_capture(&cmd.program, &cmd.args) {
                    Ok((true, lines)) => {
                        emit_output_lines(&tx, lines);
                        let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                            FeedLevel::Done,
                            "full upgrade completed",
                        )));
                        let _ = tx.send(AppEvent::InstallFinished {
                            installed: 0,
                            skipped: 0,
                            failed: 0,
                            aborted: false,
                        });
                        return;
                    }
                    Ok((false, lines)) => {
                        emit_output_lines(&tx, lines);
                        let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                            FeedLevel::Error,
                            format!("full upgrade failed via {}", backend.bin()),
                        )));
                    }
                    Err(err) => {
                        let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                            FeedLevel::Error,
                            format!("{}", err),
                        )));
                    }
                }
            }

            let _ = tx.send(AppEvent::InstallFinished {
                installed: 0,
                skipped: 0,
                failed: 1,
                aborted: false,
            });
        })
        .await;
    });
}

fn run_install(
    queue: Vec<QueueItem>,
    backend_priority: Vec<BackendId>,
    backend_available: HashMap<BackendId, bool>,
    dry_run: bool,
    tx: UnboundedSender<AppEvent>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
) -> Result<()> {
    preflight()?;

    let backends = create_backends();
    let mut installed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    if !dry_run {
        // Cache sudo credentials once so pacman path is non-interactive later.
        let _ = run_command_capture("sudo", &["-v".to_string()]);
    }

    for (index, item) in queue.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                FeedLevel::Warning,
                "abort requested; stopping queue",
            )));
            let _ = tx.send(AppEvent::InstallFinished {
                installed,
                skipped,
                failed,
                aborted: true,
            });
            return Ok(());
        }

        while pause.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(120));
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }

        let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
            FeedLevel::Resolve,
            format!("resolving {} ({}/{})", item.name, index + 1, queue.len()),
        )));

        if item.action == QueueAction::Install && package_installed(&item.name) {
            skipped += 1;
            let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                FeedLevel::Info,
                format!("skip {} (already installed)", item.name),
            )));
            continue;
        }

        let mut success = false;
        for backend in &backend_priority {
            if !backend_available.get(backend).copied().unwrap_or(false) {
                continue;
            }

            let Some(handler) = backends.get(backend) else {
                continue;
            };

            let cmd = command_for_action(handler.as_ref(), item);
            if dry_run {
                let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                    FeedLevel::Info,
                    format!("dry-run: {}", cmd.as_debug_string()),
                )));
                success = true;
                break;
            }

            let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                FeedLevel::Active,
                format!(
                    "{} {} via {}",
                    action_name(item.action),
                    item.name,
                    backend.bin()
                ),
            )));

            match run_command_capture(&cmd.program, &cmd.args) {
                Ok((true, lines)) => {
                    emit_output_lines(&tx, lines);
                    success = true;
                    break;
                }
                Ok((false, lines)) => {
                    emit_output_lines(&tx, lines);
                    let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                        FeedLevel::Error,
                        format!("{} failed via {}", item.name, backend.bin()),
                    )));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                        FeedLevel::Error,
                        format!("command error via {}: {err}", backend.bin()),
                    )));
                }
            }
        }

        if success {
            installed += 1;
            let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                FeedLevel::Done,
                format!("done {}", item.name),
            )));
        } else {
            failed += 1;
            let _ = tx.send(AppEvent::InstallLine(FeedItem::new(
                FeedLevel::Error,
                format!("failed {}", item.name),
            )));
        }
    }

    let _ = tx.send(AppEvent::InstallFinished {
        installed,
        skipped,
        failed,
        aborted: false,
    });
    Ok(())
}

fn command_for_action(
    backend: &dyn PackageBackend,
    item: &QueueItem,
) -> crate::backend::CommandSpec {
    let packages = vec![item.name.clone()];
    match item.action {
        QueueAction::Install => backend.install_cmd(&packages),
        QueueAction::Remove => backend.remove_cmd(&packages),
    }
}

fn action_name(action: QueueAction) -> &'static str {
    match action {
        QueueAction::Install => "installing",
        QueueAction::Remove => "removing",
    }
}

fn emit_output_lines(tx: &UnboundedSender<AppEvent>, lines: Vec<String>) {
    for line in lines
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .take(25)
    {
        let _ = tx.send(AppEvent::InstallLine(FeedItem::new(FeedLevel::Info, line)));
    }
}

fn package_installed(name: &str) -> bool {
    Command::new("pacman")
        .args(["-Q", name])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_command_capture(program: &str, args: &[String]) -> Result<(bool, Vec<String>)> {
    let out = Command::new(program).args(args).output()?;
    let mut lines = Vec::new();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    lines.extend(stdout.lines().map(ToString::to_string));
    lines.extend(stderr.lines().map(ToString::to_string));

    Ok((out.status.success(), lines))
}

fn preflight() -> Result<()> {
    if !std::path::Path::new("/etc/arch-release").exists() {
        anyhow::bail!("this app is for Arch Linux only");
    }

    let uid_output = Command::new("id").arg("-u").output()?;
    if String::from_utf8_lossy(&uid_output.stdout).trim() == "0" {
        anyhow::bail!("run as a non-root user");
    }

    let addr = ("archlinux.org", 443)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("failed to resolve archlinux.org"))?;
    TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("internet check failed (tcp 443 archlinux.org)"))
}
