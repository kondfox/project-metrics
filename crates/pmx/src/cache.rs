//! `.pmx/cache.sqlite`: per-repo ingest results keyed by `(repo, tip SHA, settings hash)`, so an
//! unchanged repo is not re-read (plan §2.2).

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::ingest::RepoIngest;

pub struct Cache {
    db: Connection,
}

impl Cache {
    pub fn open(path: &Path) -> Result<Cache> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let db = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS repo_ingest (
                repo TEXT NOT NULL,
                sha TEXT NOT NULL,
                settings TEXT NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (repo, sha, settings)
            );",
        )?;
        Ok(Cache { db })
    }

    pub fn get(&self, repo: &str, sha: &str, settings: &str) -> Result<Option<RepoIngest>> {
        let data: Option<String> = self
            .db
            .query_row(
                "SELECT data FROM repo_ingest WHERE repo = ?1 AND sha = ?2 AND settings = ?3",
                params![repo, sha, settings],
                |r| r.get(0),
            )
            .optional()?;
        // An entry that no longer parses (older pmx) is a miss.
        Ok(data.and_then(|d| serde_json::from_str(&d).ok()))
    }

    /// Store the result for this tip, dropping older entries of the same repo.
    pub fn put(&self, repo: &str, sha: &str, settings: &str, ingest: &RepoIngest) -> Result<()> {
        let data = serde_json::to_string(ingest)?;
        self.db
            .execute("DELETE FROM repo_ingest WHERE repo = ?1", params![repo])?;
        self.db.execute(
            "INSERT INTO repo_ingest (repo, sha, settings, data) VALUES (?1, ?2, ?3, ?4)",
            params![repo, sha, settings, data],
        )?;
        Ok(())
    }
}
