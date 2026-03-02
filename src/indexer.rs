use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{Context, Result};

use crate::model::{Manager, ManagerAvailability, PackageRecord, RepoHint};

pub enum IndexEvent {
    Status(String),
    Ready {
        packages: Vec<PackageRecord>,
        official_set: HashSet<String>,
        availability: ManagerAvailability,
    },
    Error(String),
}

pub fn spawn_indexer(tx: Sender<IndexEvent>) {
    thread::spawn(move || {
        if let Err(err) = run_indexer(tx.clone()) {
            let _ = tx.send(IndexEvent::Error(err.to_string()));
        }
    });
}

fn run_indexer(tx: Sender<IndexEvent>) -> Result<()> {
    let _ = tx.send(IndexEvent::Status("Detecting managers...".to_string()));

    let availability = ManagerAvailability {
        pacman: command_exists("pacman"),
        yay: command_exists("yay"),
        paru: command_exists("paru"),
    };

    if !availability.pacman {
        anyhow::bail!("pacman was not found in PATH");
    }

    let _ = tx.send(IndexEvent::Status(
        "Loading official package index from pacman...".to_string(),
    ));
    let official_names =
        command_lines("pacman", &["-Slq"]).context("failed to read pacman package list")?;

    let mut merged: HashMap<String, RepoHint> = HashMap::with_capacity(official_names.len() * 2);
    let mut official_set: HashSet<String> = HashSet::with_capacity(official_names.len() * 2);

    for name in official_names {
        if name.is_empty() {
            continue;
        }
        official_set.insert(name.clone());
        merged.insert(name, RepoHint::Official);
    }

    let aur_source = if availability.yay {
        Some(Manager::Yay)
    } else if availability.paru {
        Some(Manager::Paru)
    } else {
        None
    };

    if let Some(manager) = aur_source {
        let _ = tx.send(IndexEvent::Status(format!(
            "Loading AUR package index via {}...",
            manager.bin()
        )));

        let aur_names = command_lines(manager.bin(), &["-Slqa"]).with_context(|| {
            format!(
                "failed to read AUR package list via {} -Slqa",
                manager.bin()
            )
        })?;

        for name in aur_names {
            if name.is_empty() {
                continue;
            }
            match merged.get_mut(&name) {
                Some(repo) => {
                    if matches!(*repo, RepoHint::Official) {
                        *repo = RepoHint::Both;
                    }
                }
                None => {
                    merged.insert(name, RepoHint::Aur);
                }
            }
        }
    } else {
        let _ = tx.send(IndexEvent::Status(
            "No AUR helper detected; browse will include official packages only".to_string(),
        ));
    }

    let _ = tx.send(IndexEvent::Status(
        "Finalizing searchable package database...".to_string(),
    ));

    let mut packages: Vec<PackageRecord> = merged
        .into_iter()
        .map(|(name, repo)| PackageRecord {
            lower: name.to_lowercase(),
            name,
            repo,
        })
        .collect();

    packages.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    let _ = tx.send(IndexEvent::Status(format!(
        "Loaded {} searchable packages",
        packages.len()
    )));

    let _ = tx.send(IndexEvent::Ready {
        packages,
        official_set,
        availability,
    });

    Ok(())
}

fn command_exists(bin: &str) -> bool {
    Command::new("bash")
        .args(["-lc", &format!("command -v {bin} >/dev/null")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn command_lines(program: &str, args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run command: {} {}", program, args.join(" ")))?;

    if !output.status.success() {
        anyhow::bail!("command failed: {} {}", program, args.join(" "));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}
