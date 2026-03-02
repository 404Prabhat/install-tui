use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::config;
use crate::model::{PackageRecord, RepoKind};

pub fn db_path() -> PathBuf {
    config::cache_dir().join("pkgdb.sqlite")
}

pub fn init_db(path: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(config::cache_dir()).context("failed to create cache directory")?;
    let conn =
        Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS packages (
          name TEXT PRIMARY KEY,
          version TEXT,
          description TEXT,
          repo TEXT,
          size_bytes INTEGER,
          installed INTEGER,
          upgradable INTEGER,
          new_version TEXT,
          updated_at INTEGER,
          repo_kind TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_packages_repo ON packages(repo);
        CREATE INDEX IF NOT EXISTS idx_packages_updated ON packages(updated_at);
        "#,
    )
    .context("failed to initialize database schema")?;
    Ok(())
}

pub fn load_packages(path: &PathBuf) -> Result<Vec<PackageRecord>> {
    let conn =
        Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut stmt = conn
        .prepare(
            "SELECT name, version, description, repo, size_bytes, installed, upgradable, new_version, updated_at, repo_kind FROM packages",
        )
        .context("failed to prepare package query")?;

    let rows = stmt
        .query_map([], |row| {
            let repo_kind_str: String = row.get(9)?;
            Ok(PackageRecord {
                name: row.get(0)?,
                version: row.get(1)?,
                description: row.get(2)?,
                repo: row.get(3)?,
                size_bytes: row.get(4)?,
                installed: row.get::<_, i64>(5)? == 1,
                upgradable: row.get::<_, i64>(6)? == 1,
                new_version: row.get(7)?,
                updated_at: row.get(8)?,
                repo_kind: if repo_kind_str == "aur" {
                    RepoKind::Aur
                } else {
                    RepoKind::Official
                },
            })
        })
        .context("failed to map package rows")?;

    let mut packages = Vec::new();
    for row in rows {
        packages.push(row.context("failed to decode package row")?);
    }
    Ok(packages)
}

pub fn replace_packages(path: &PathBuf, packages: &[PackageRecord]) -> Result<()> {
    let mut conn =
        Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let tx = conn.transaction().context("failed to start transaction")?;
    tx.execute("DELETE FROM packages", [])
        .context("failed to clear package table")?;

    let mut stmt = tx
        .prepare(
            r#"
            INSERT INTO packages (
                name, version, description, repo, size_bytes, installed, upgradable,
                new_version, updated_at, repo_kind
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .context("failed to prepare insert")?;

    for pkg in packages {
        stmt.execute(params![
            pkg.name,
            pkg.version,
            pkg.description,
            pkg.repo,
            pkg.size_bytes,
            i64::from(pkg.installed),
            i64::from(pkg.upgradable),
            pkg.new_version,
            pkg.updated_at,
            if pkg.repo_kind == RepoKind::Aur {
                "aur"
            } else {
                "official"
            },
        ])
        .with_context(|| format!("failed to insert package {}", pkg.name))?;
    }

    drop(stmt);
    tx.commit().context("failed to commit package sync")?;
    Ok(())
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
