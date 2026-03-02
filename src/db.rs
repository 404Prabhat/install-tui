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
    config::ensure_dirs()?;
    let conn = Connection::open(path).with_context(|| format!("open db {}", path.display()))?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS packages (
          name TEXT PRIMARY KEY,
          version TEXT NOT NULL,
          description TEXT NOT NULL,
          repo TEXT NOT NULL,
          size_bytes INTEGER NOT NULL,
          installed INTEGER NOT NULL,
          upgradable INTEGER NOT NULL,
          new_version TEXT,
          updated_at INTEGER NOT NULL,
          repo_kind TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_packages_repo ON packages(repo);
        CREATE INDEX IF NOT EXISTS idx_packages_updated_at ON packages(updated_at);
        ",
    )
    .context("create schema")?;

    Ok(())
}

pub fn load_packages(path: &PathBuf) -> Result<Vec<PackageRecord>> {
    init_db(path)?;
    let conn = Connection::open(path).with_context(|| format!("open db {}", path.display()))?;
    let mut stmt = conn
        .prepare(
            "SELECT name, version, description, repo, size_bytes, installed, upgradable, new_version, updated_at, repo_kind
             FROM packages
             ORDER BY name",
        )
        .context("prepare load packages")?;

    let rows = stmt
        .query_map([], |row| {
            let repo_kind = match row.get::<_, String>(9)?.as_str() {
                "aur" => RepoKind::Aur,
                _ => RepoKind::Official,
            };
            Ok(PackageRecord {
                name: row.get(0)?,
                version: row.get(1)?,
                description: row.get(2)?,
                repo: row.get(3)?,
                size_bytes: row.get(4)?,
                installed: row.get::<_, i64>(5)? != 0,
                upgradable: row.get::<_, i64>(6)? != 0,
                new_version: row.get(7)?,
                updated_at: row.get(8)?,
                repo_kind,
            })
        })
        .context("query packages")?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("decode package row")?);
    }
    Ok(out)
}

pub fn replace_packages(path: &PathBuf, packages: &[PackageRecord]) -> Result<()> {
    init_db(path)?;
    let mut conn = Connection::open(path).with_context(|| format!("open db {}", path.display()))?;
    let tx = conn.transaction().context("begin tx")?;

    tx.execute("DELETE FROM packages", []).context("clear table")?;

    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO packages (
                    name, version, description, repo, size_bytes,
                    installed, upgradable, new_version, updated_at, repo_kind
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .context("prepare insert")?;

        for pkg in packages {
            let repo_kind = match pkg.repo_kind {
                RepoKind::Official => "official",
                RepoKind::Aur => "aur",
            };

            stmt.execute(params![
                pkg.name,
                pkg.version,
                pkg.description,
                pkg.repo,
                pkg.size_bytes,
                if pkg.installed { 1 } else { 0 },
                if pkg.upgradable { 1 } else { 0 },
                pkg.new_version,
                pkg.updated_at,
                repo_kind,
            ])
            .with_context(|| format!("insert package {}", pkg.name))?;
        }
    }

    tx.commit().context("commit tx")?;
    Ok(())
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
