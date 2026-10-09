//! `.pmx/cache.sqlite` (plan §2.2): each repo's stage results keyed by `(repo, stage, settings)`
//! with the tip SHA they were computed at, plus per-stage timings for the ETA.
//!
//! A result is written as soon as its repo finishes the stage, so an interrupted run loses at
//! most the stages in flight. The next run continues from the stored SHA when the new tip descends
//! from it.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub struct Cache {
    db: Connection,
}

impl Cache {
    pub fn open(path: &Path) -> Result<Cache> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let db = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        db.busy_timeout(Duration::from_secs(30))?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             DROP TABLE IF EXISTS repo_ingest;
             CREATE TABLE IF NOT EXISTS stage_result (
                repo TEXT NOT NULL,
                stage TEXT NOT NULL,
                settings TEXT NOT NULL,
                sha TEXT NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (repo, stage, settings)
             );
             CREATE TABLE IF NOT EXISTS timing (
                stage TEXT NOT NULL,
                units INTEGER NOT NULL,
                seconds REAL NOT NULL
             );",
        )?;
        Ok(Cache { db })
    }

    /// The stored result and the SHA it was computed at. An entry that no longer parses (older
    /// pmx) is a miss.
    pub fn get<T: DeserializeOwned>(&self, repo: &str, stage: &str, settings: &str) -> Result<Option<(String, T)>> {
        let row: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT sha, data FROM stage_result WHERE repo = ?1 AND stage = ?2 AND settings = ?3",
                params![repo, stage, settings],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row.and_then(|(sha, data)| serde_json::from_str(&data).ok().map(|v| (sha, v))))
    }

    pub fn put<T: Serialize>(&self, repo: &str, stage: &str, settings: &str, sha: &str, value: &T) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO stage_result (repo, stage, settings, sha, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![repo, stage, settings, sha, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn record_timing(&self, stage: &str, units: u64, seconds: f64) -> Result<()> {
        if units > 0 {
            self.db.execute(
                "INSERT INTO timing (stage, units, seconds) VALUES (?1, ?2, ?3)",
                params![stage, units as i64, seconds],
            )?;
        }
        Ok(())
    }

    /// Seconds per unit over the stage's last 50 measurements.
    pub fn cost(&self, stage: &str) -> Result<Option<f64>> {
        let (units, secs): (Option<i64>, Option<f64>) = self.db.query_row(
            "SELECT SUM(units), SUM(seconds) FROM
               (SELECT units, seconds FROM timing WHERE stage = ?1 ORDER BY rowid DESC LIMIT 50)",
            params![stage],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(match (units, secs) {
            (Some(u), Some(s)) if u > 0 => Some(s / u as f64),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_results_and_costs() {
        let tmp = tempfile::tempdir().unwrap();
        let c = Cache::open(&tmp.path().join("c.sqlite")).unwrap();
        assert!(c.get::<Vec<u32>>("api", "ingest", "k").unwrap().is_none());
        c.put("api", "ingest", "k", "abc", &vec![1u32, 2]).unwrap();
        c.put("api", "ingest", "k", "def", &vec![3u32]).unwrap();
        assert_eq!(
            c.get::<Vec<u32>>("api", "ingest", "k").unwrap(),
            Some(("def".into(), vec![3]))
        );
        assert!(c.get::<Vec<u32>>("api", "ingest", "other").unwrap().is_none());
        // A shape change is a miss, not an error.
        assert!(c.get::<String>("api", "ingest", "k").unwrap().is_none());

        assert_eq!(c.cost("ingest").unwrap(), None);
        c.record_timing("ingest", 1000, 0.5).unwrap();
        c.record_timing("ingest", 3000, 2.5).unwrap();
        c.record_timing("ingest", 0, 9.0).unwrap();
        assert_eq!(c.cost("ingest").unwrap(), Some(0.00075));
    }
}
