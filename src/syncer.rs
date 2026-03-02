use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::db;
use crate::model::{AppEvent, PackageRecord, RepoKind};

pub async fn run_full_sync(path: PathBuf, tx: UnboundedSender<AppEvent>) {
    let _ = tx.send(AppEvent::SyncStarted);
    let tx_done = tx.clone();

    let result = tokio::task::spawn_blocking(move || sync_blocking(&path)).await;
    match result {
        Ok(Ok(packages)) => {
            let _ = tx_done.send(AppEvent::SyncFinished { packages });
        }
        Ok(Err(err)) => {
            let _ = tx_done.send(AppEvent::SyncError(err.to_string()));
        }
        Err(err) => {
            let _ = tx_done.send(AppEvent::SyncError(format!("sync task join error: {err}")));
        }
    }
}

fn sync_blocking(path: &PathBuf) -> Result<Vec<PackageRecord>> {
    let mut packages = if command_exists("expac") {
        parse_expac()?
    } else {
        parse_pacman_sl()?
    };

    let installed = command_lines("pacman", &["-Qq"]).unwrap_or_default();
    let installed_set: HashSet<String> = installed.into_iter().collect();

    let upgrades = command_lines("pacman", &["-Qu"]).unwrap_or_default();
    let mut upgrade_map = HashMap::new();
    for line in upgrades {
        // Format usually: name old -> new
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() >= 2 {
            let name = parts[0].to_string();
            let new_ver = parts.last().unwrap_or(&"").to_string();
            if !name.is_empty() && !new_ver.is_empty() {
                upgrade_map.insert(name, new_ver);
            }
        }
    }

    let now = db::now_ts();
    for pkg in &mut packages {
        pkg.installed = installed_set.contains(&pkg.name);
        if let Some(new_ver) = upgrade_map.get(&pkg.name) {
            pkg.upgradable = true;
            pkg.new_version = Some(new_ver.clone());
        }
        pkg.updated_at = now;
    }

    db::replace_packages(path, &packages)?;
    Ok(packages)
}

fn parse_expac() -> Result<Vec<PackageRecord>> {
    let out = Command::new("expac")
        .args(["-S", "%n\t%v\t%d\t%s\t%r"])
        .output()
        .context("failed to run expac")?;
    if !out.status.success() {
        anyhow::bail!("expac failed")
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut out = Vec::new();
    for line in text.lines() {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() < 5 {
            continue;
        }
        let size = parts[3].parse::<i64>().unwrap_or(0);
        out.push(PackageRecord {
            name: parts[0].to_string(),
            version: parts[1].to_string(),
            description: parts[2].to_string(),
            size_bytes: size,
            repo: parts[4].to_string(),
            installed: false,
            upgradable: false,
            new_version: None,
            updated_at: 0,
            repo_kind: RepoKind::Official,
        });
    }
    Ok(out)
}

fn parse_pacman_sl() -> Result<Vec<PackageRecord>> {
    let out = Command::new("pacman")
        .args(["-Sl"])
        .output()
        .context("failed to run pacman -Sl")?;
    if !out.status.success() {
        anyhow::bail!("pacman -Sl failed")
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut pkgs = Vec::new();
    for line in text.lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }

        pkgs.push(PackageRecord {
            repo: parts[0].to_string(),
            name: parts[1].to_string(),
            version: parts[2].to_string(),
            description: String::new(),
            size_bytes: 0,
            installed: false,
            upgradable: false,
            new_version: None,
            updated_at: 0,
            repo_kind: RepoKind::Official,
        });
    }
    Ok(pkgs)
}

fn command_exists(bin: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {bin} >/dev/null")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn command_lines(program: &str, args: &[&str]) -> Result<Vec<String>> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {} {}", program, args.join(" ")))?;
    if !out.status.success() {
        anyhow::bail!("command failed: {} {}", program, args.join(" "));
    }

    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

#[derive(Debug, Deserialize)]
struct AurResponse {
    results: Vec<AurRecord>,
}

#[derive(Debug, Deserialize)]
struct AurRecord {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Description")]
    description: Option<String>,
}

pub async fn query_aur_if_needed(query: String, tx: UnboundedSender<AppEvent>) {
    if query.len() < 3 {
        return;
    }

    let url = format!("https://aur.archlinux.org/rpc/v5/search/{}?by=name", query);

    let response = match reqwest::get(&url).await {
        Ok(r) => r,
        Err(err) => {
            let _ = tx.send(AppEvent::SyncError(format!("AUR request failed: {err}")));
            return;
        }
    };

    let parsed: AurResponse = match response.json().await {
        Ok(p) => p,
        Err(err) => {
            let _ = tx.send(AppEvent::SyncError(format!("AUR parse failed: {err}")));
            return;
        }
    };

    let now = db::now_ts();
    let packages = parsed
        .results
        .into_iter()
        .map(|row| PackageRecord {
            name: row.name,
            version: row.version,
            description: row.description.unwrap_or_default(),
            repo: "aur".to_string(),
            size_bytes: 0,
            installed: false,
            upgradable: false,
            new_version: None,
            updated_at: now,
            repo_kind: RepoKind::Aur,
        })
        .collect::<Vec<_>>();

    let _ = tx.send(AppEvent::AurResults { query, packages });
}
