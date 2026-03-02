use std::process::Command;

use tokio::sync::mpsc::UnboundedSender;

use crate::model::{AppEvent, PackageDetail};

pub async fn fetch_package_detail(package: String, tx: UnboundedSender<AppEvent>) {
    let pkg = package.clone();
    let result = tokio::task::spawn_blocking(move || load_detail_blocking(&pkg)).await;

    match result {
        Ok(Ok(rendered)) => {
            let _ = tx.send(AppEvent::DetailLoaded(PackageDetail { package, rendered }));
        }
        Ok(Err(err)) => {
            let _ = tx.send(AppEvent::DetailError {
                package,
                error: err,
            });
        }
        Err(err) => {
            let _ = tx.send(AppEvent::DetailError {
                package,
                error: err.to_string(),
            });
        }
    }
}

fn load_detail_blocking(package: &str) -> Result<String, String> {
    let commands = vec![
        ("pacman", vec!["-Si", package]),
        ("yay", vec!["-Si", package]),
        ("paru", vec!["-Si", package]),
        ("aura", vec!["-Ai", package]),
        ("trizen", vec!["-Si", package]),
    ];

    for (program, args) in commands {
        let out = Command::new(program).args(args).output();
        let Ok(output) = out else {
            continue;
        };

        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let rendered = normalize_detail_text(package, &text);
            return Ok(rendered);
        }
    }

    Err("no backend could fetch details for package".to_string())
}

fn normalize_detail_text(package: &str, raw: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!("{}", package));
    lines.push("────────────────────────────".to_string());

    let mut description = String::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let value = v.trim();
            if !key.is_empty() && !value.is_empty() {
                lines.push(format!("{:<12} {}", format!("{}:", key), value));
            }

            if key.eq_ignore_ascii_case("Description") {
                description = value.to_string();
            }
        }
    }

    if !description.is_empty() {
        lines.push(String::new());
        lines.push("Description:".to_string());
        lines.push(description);
    }

    if lines.len() <= 2 {
        lines.push(raw.to_string());
    }

    lines.join("\n")
}
