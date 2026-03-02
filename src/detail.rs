use std::process::Command;

use tokio::sync::mpsc::UnboundedSender;

use crate::model::{AppEvent, PackageDetail};

pub fn spawn_detail_fetch(package: String, tx: UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let package_for_blocking = package.clone();
        let output =
            tokio::task::spawn_blocking(move || fetch_blocking(&package_for_blocking)).await;

        match output {
            Ok(Ok(lines)) => {
                let _ = tx.send(AppEvent::DetailLoaded(PackageDetail { package, lines }));
            }
            Ok(Err(err)) => {
                let _ = tx.send(AppEvent::DetailError(err));
            }
            Err(err) => {
                let _ = tx.send(AppEvent::DetailError(format!("detail task failed: {err}")));
            }
        }
    });
}

fn fetch_blocking(package: &str) -> Result<Vec<String>, String> {
    let candidates = [
        ("pacman", vec!["-Si", package]),
        ("yay", vec!["-Si", package]),
        ("paru", vec!["-Si", package]),
        ("aura", vec!["-Ai", package]),
        ("trizen", vec!["-Si", package]),
    ];

    for (program, args) in candidates {
        if !command_exists(program) {
            continue;
        }
        match Command::new(program).args(args).output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut lines = text
                    .lines()
                    .map(str::trim_end)
                    .filter(|line| !line.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                if lines.is_empty() {
                    lines.push("No package details returned".to_string());
                }
                return Ok(lines);
            }
            Ok(_) => continue,
            Err(_) => continue,
        }
    }

    Err(format!("failed to fetch package details for {package}"))
}

fn command_exists(bin: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {bin} >/dev/null")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
