use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::db;
use crate::model::{AppEvent, FeedLevel, PackageRecord, RepoKind};

pub async fn run_full_sync(db_path: PathBuf, tx: UnboundedSender<AppEvent>) {
    let _ = tx.send(AppEvent::SyncStarted);
    let _ = tx.send(AppEvent::Feed {
        level: FeedLevel::Info,
        message: "sync: refreshing package metadata".to_string(),
    });

    let result = tokio::task::spawn_blocking(move || sync_blocking(&db_path)).await;

    match result {
        Ok(Ok(packages)) => {
            let count = packages.len();
            let _ = tx.send(AppEvent::SyncFinished { count, packages });
            let _ = tx.send(AppEvent::Feed {
                level: FeedLevel::Success,
                message: format!("sync: done ({} packages)", count),
            });
        }
        Ok(Err(err)) => {
            let _ = tx.send(AppEvent::SyncError(err.to_string()));
            let _ = tx.send(AppEvent::Feed {
                level: FeedLevel::Error,
                message: format!("sync failed: {err}"),
            });
        }
        Err(err) => {
            let _ = tx.send(AppEvent::SyncError(err.to_string()));
            let _ = tx.send(AppEvent::Feed {
                level: FeedLevel::Error,
                message: format!("sync join error: {err}"),
            });
        }
    }
}

fn sync_blocking(db_path: &PathBuf) -> Result<Vec<PackageRecord>> {
    let mut packages = if command_exists("expac") {
        parse_expac_output(&run_output("expac", &["-S", "%n\t%v\t%d\t%s\t%r"])? )?
    } else {
        parse_pacman_fallback(&run_output("pacman", &["-Sl"])? )?
    };

    let installed = parse_name_set(&run_output("pacman", &["-Qq"])?);
    let upgrades = parse_upgrades(&run_output_allow_fail("pacman", &["-Qu"])?.unwrap_or_default());

    let now = db::now_ts();
    for pkg in &mut packages {
        pkg.installed = installed.contains(&pkg.name);
        pkg.upgradable = upgrades.contains_key(&pkg.name);
        pkg.new_version = upgrades.get(&pkg.name).cloned();
        pkg.updated_at = now;
    }

    db::replace_packages(db_path, &packages)?;
    Ok(packages)
}

fn parse_expac_output(text: &str) -> Result<Vec<PackageRecord>> {
    let mut out = Vec::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let cols = line.split('\t').collect::<Vec<_>>();
        if cols.len() < 5 {
            continue;
        }

        let size = cols[3].trim().parse::<i64>().unwrap_or(0);
        out.push(PackageRecord {
            name: cols[0].trim().to_string(),
            version: cols[1].trim().to_string(),
            description: cols[2].trim().to_string(),
            size_bytes: size,
            repo: cols[4].trim().to_string(),
            installed: false,
            upgradable: false,
            new_version: None,
            updated_at: 0,
            repo_kind: RepoKind::Official,
        });
    }

    Ok(out)
}

fn parse_pacman_fallback(text: &str) -> Result<Vec<PackageRecord>> {
    let mut out = Vec::new();
    for line in text.lines() {
        let cols = line.split_whitespace().collect::<Vec<_>>();
        if cols.len() < 3 {
            continue;
        }
        let repo = cols[0].to_string();
        let name = cols[1].to_string();
        let version = cols[2].to_string();
        let description = if cols.len() > 3 {
            cols[3..].join(" ")
        } else {
            String::new()
        };
        out.push(PackageRecord {
            name,
            version,
            description,
            size_bytes: 0,
            repo,
            installed: false,
            upgradable: false,
            new_version: None,
            updated_at: 0,
            repo_kind: RepoKind::Official,
        });
    }
    Ok(out)
}

fn parse_name_set(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_upgrades(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let cols = line.split_whitespace().collect::<Vec<_>>();
        if cols.len() >= 4 {
            out.insert(cols[0].to_string(), cols[3].to_string());
        } else if cols.len() >= 2 {
            out.insert(cols[0].to_string(), cols[1].to_string());
        }
    }
    out
}

fn run_output(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run: {} {}", program, args.join(" ")))?;

    if !out.status.success() {
        anyhow::bail!("non-zero exit: {} {}", program, args.join(" "));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_output_allow_fail(program: &str, args: &[&str]) -> Result<Option<String>> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run: {} {}", program, args.join(" ")))?;

    if !out.status.success() {
        return Ok(None);
    }

    Ok(Some(String::from_utf8_lossy(&out.stdout).to_string()))
}

fn command_exists(bin: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {bin} >/dev/null")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
struct AurResponse {
    #[serde(default)]
    results: Vec<AurItem>,
}

#[derive(Debug, Deserialize)]
struct AurItem {
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
    let response = reqwest::get(url).await;

    let result = match response {
        Ok(resp) => resp.json::<AurResponse>().await,
        Err(err) => {
            let _ = tx.send(AppEvent::Feed {
                level: FeedLevel::Warn,
                message: format!("aur query failed: {}", err),
            });
            return;
        }
    };

    match result {
        Ok(parsed) => {
            let now = db::now_ts();
            let packages = parsed
                .results
                .into_iter()
                .map(|item| PackageRecord {
                    name: item.name,
                    version: item.version,
                    description: item.description.unwrap_or_default(),
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
        Err(err) => {
            let _ = tx.send(AppEvent::Feed {
                level: FeedLevel::Warn,
                message: format!("aur parse failed: {}", err),
            });
        }
    }
}
