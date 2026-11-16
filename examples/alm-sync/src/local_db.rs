use anyhow::{Context, Result, ensure};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;

// Fields are populated by sqlx::Row::get and consumed by callers via destructuring.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AlarmRecord {
    pub id: Option<i64>,
    pub tag_name: String,
    pub alarm_type: String,
    pub datetime: i64,
    pub mseconds: i32,
    pub value: i64,
    pub comment: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UploadLogEntry {
    pub id: i64,
    pub alarm_id: i64,
    pub record: AlarmRecord,
    pub target_db: String,
    pub status: String,
    pub retry_count: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct QueryCheckpoint {
    pub tag_name: String,
    pub alarm_type: String,
    pub last_end_time: i64,
}

/// Thin wrapper around `SqlitePool`. All operations are async — sync callers
/// should use `tokio::runtime::Handle::block_on`.
#[derive(Clone)]
pub struct LocalDb {
    pool: SqlitePool,
}

impl LocalDb {
    pub async fn open(
        path: &Path,
        source: &str,
        destination: Option<&str>,
        adopt_existing: bool,
    ) -> Result<Self> {
        let options = SqliteConnectOptions::new().foreign_keys(true);
        let options = if path == Path::new(":memory:") {
            options.in_memory(true)
        } else {
            options
                .filename(std::path::absolute(path).context("Invalid SQLite filename")?)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
        };
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("Failed to open SQLite database")?;

        let db = Self { pool };
        db.migrate(source, destination, adopt_existing).await?;
        Ok(db)
    }

    async fn migrate(
        &self,
        source: &str,
        destination: Option<&str>,
        adopt_existing: bool,
    ) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS alarms (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                tag_name    TEXT NOT NULL,
                alarm_type  TEXT NOT NULL,
                datetime    INTEGER NOT NULL,
                mseconds    INTEGER NOT NULL DEFAULT 0,
                value       INTEGER NOT NULL DEFAULT 0,
                comment     TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(tag_name, alarm_type, datetime, mseconds)
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS upload_log (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                alarm_id        INTEGER NOT NULL,
                target_db       TEXT    NOT NULL,
                status          TEXT    NOT NULL DEFAULT 'pending',
                error_message   TEXT,
                retry_count     INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
                updated_at      TEXT    NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (alarm_id) REFERENCES alarms(id) ON DELETE CASCADE
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_upload_log_status
             ON upload_log(status, target_db)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS query_checkpoint (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                tag_name        TEXT NOT NULL,
                alarm_type      TEXT NOT NULL,
                last_end_time   INTEGER NOT NULL,
                updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(tag_name, alarm_type)
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sync_identity (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                source TEXT NOT NULL,
                destination TEXT
            )",
        )
        .execute(&mut *tx)
        .await?;
        let binding = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT source, destination FROM sync_identity WHERE id = 1",
        )
        .fetch_optional(&mut *tx)
        .await?;
        match binding {
            Some((stored_source, stored_destination)) => {
                ensure!(
                    stored_source == source,
                    "SQLite state belongs to another SCADA source or user"
                );
                if let Some(destination) = destination {
                    if let Some(stored) = stored_destination {
                        ensure!(
                            stored == destination,
                            "SQLite state belongs to another upload destination"
                        );
                    } else {
                        sqlx::query("UPDATE sync_identity SET destination = ?1 WHERE id = 1")
                            .bind(destination)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
            }
            None => {
                let has_rows: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM alarms)
                         OR EXISTS (SELECT 1 FROM upload_log)
                         OR EXISTS (SELECT 1 FROM query_checkpoint)",
                )
                .fetch_one(&mut *tx)
                .await?;
                ensure!(
                    !has_rows || adopt_existing,
                    "SQLite state has no recorded origin. Verify its source and destination before using --adopt-existing-state, or choose a new file"
                );
                // Existing queued rows cannot be assigned to an unknown
                // destination. A local-only archive can be adopted separately.
                if destination.is_none() {
                    let has_uploads: bool =
                        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM upload_log)")
                            .fetch_one(&mut *tx)
                            .await?;
                    ensure!(
                        !has_uploads,
                        "Configure the original upload destination before adopting this state"
                    );
                }
                sqlx::query(
                    "INSERT INTO sync_identity (id, source, destination) VALUES (1, ?1, ?2)",
                )
                .bind(source)
                .bind(destination)
                .execute(&mut *tx)
                .await?;
                if has_rows {
                    // The previous schema allowed duplicate queue rows.
                    // Keep completed work, then the most retryable row.
                    sqlx::query(
                        "DELETE FROM upload_log WHERE id IN (
                            SELECT id FROM (
                                SELECT id, ROW_NUMBER() OVER (
                                    PARTITION BY alarm_id, target_db
                                    ORDER BY CASE status WHEN 'uploaded' THEN 0 ELSE 1 END, retry_count, id
                                ) AS position FROM upload_log
                            ) WHERE position > 1
                        )",
                    )
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS uniq_upload_log_alarm_target
             ON upload_log(alarm_id, target_db)",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Persist a complete query window and its progress as one transaction.
    pub async fn commit_window(
        &self,
        tag_name: &str,
        alarm_type: &str,
        records: &[AlarmRecord],
        target_db: Option<&str>,
        end_time: i64,
    ) -> Result<usize> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut inserted = 0;
        for record in records {
            let result = sqlx::query(
                "INSERT INTO alarms (tag_name, alarm_type, datetime, mseconds, value, comment)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(tag_name, alarm_type, datetime, mseconds) DO NOTHING",
            )
            .bind(tag_name)
            .bind(alarm_type)
            .bind(record.datetime)
            .bind(record.mseconds)
            .bind(record.value)
            .bind(&record.comment)
            .execute(&mut *tx)
            .await?;
            inserted += result.rows_affected() as usize;

            if let Some(target_db) = target_db {
                sqlx::query(
                    "INSERT INTO upload_log (alarm_id, target_db, status)
                     SELECT id, ?1, 'pending' FROM alarms
                     WHERE tag_name = ?2 AND alarm_type = ?3 AND datetime = ?4 AND mseconds = ?5
                     ON CONFLICT(alarm_id, target_db) DO NOTHING",
                )
                .bind(target_db)
                .bind(tag_name)
                .bind(alarm_type)
                .bind(record.datetime)
                .bind(record.mseconds)
                .execute(&mut *tx)
                .await?;
            }
        }

        sqlx::query(
            "INSERT INTO query_checkpoint (tag_name, alarm_type, last_end_time, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(tag_name, alarm_type) DO UPDATE SET
                last_end_time = excluded.last_end_time,
                updated_at = datetime('now')
             WHERE excluded.last_end_time > query_checkpoint.last_end_time",
        )
        .bind(tag_name)
        .bind(alarm_type)
        .bind(end_time)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(inserted)
    }

    /// Fetch pending uploads.
    pub async fn get_pending_uploads(
        &self,
        target_db: &str,
        limit: usize,
        max_retry: u32,
    ) -> Result<Vec<UploadLogEntry>> {
        let rows = sqlx::query(
            "SELECT ul.id, ul.alarm_id, ul.target_db, ul.status, ul.retry_count,
                    a.id, a.tag_name, a.alarm_type, a.datetime, a.mseconds, a.value, a.comment
             FROM upload_log ul
             JOIN alarms a ON a.id = ul.alarm_id
             WHERE ul.status IN ('pending', 'failed')
               AND ul.target_db = ?1
               AND ul.retry_count <= ?2
             ORDER BY ul.id
             LIMIT ?3",
        )
        .bind(target_db)
        .bind(max_retry)
        .bind(i64::try_from(limit).context("Upload batch size is too large")?)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                Ok(UploadLogEntry {
                    id: row.get(0),
                    alarm_id: row.get(1),
                    target_db: row.get(2),
                    status: row.get(3),
                    retry_count: row.get(4),
                    record: AlarmRecord {
                        id: Some(row.get(5)),
                        tag_name: row.get(6),
                        alarm_type: row.get(7),
                        datetime: row.get(8),
                        mseconds: row.get(9),
                        value: row.get(10),
                        comment: row.get(11),
                    },
                })
            })
            .collect()
    }

    /// Mark a batch of upload_log entries as 'uploaded'.
    pub async fn mark_uploaded(&self, log_ids: &[i64]) -> Result<()> {
        if log_ids.is_empty() {
            return Ok(());
        }
        let placeholders: Vec<String> = log_ids.iter().map(|_| "?".into()).collect();
        let sql = format!(
            "UPDATE upload_log SET status='uploaded', updated_at=datetime('now') WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut query = sqlx::query(&sql);
        for id in log_ids {
            query = query.bind(id);
        }
        query.execute(&self.pool).await?;
        Ok(())
    }

    /// Mark a batch of upload_log entries as 'failed'.
    pub async fn mark_failed(&self, log_ids: &[i64], error: &str) -> Result<()> {
        if log_ids.is_empty() {
            return Ok(());
        }
        let placeholders: Vec<String> = log_ids.iter().map(|_| "?".into()).collect();
        let sql = format!(
            "UPDATE upload_log SET status='failed', error_message=?1, retry_count=retry_count+1,
                    updated_at=datetime('now') WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut query = sqlx::query(&sql).bind(error);
        for id in log_ids {
            query = query.bind(id);
        }
        query.execute(&self.pool).await?;
        Ok(())
    }

    /// Get the checkpoint for a given tag/type pair.
    pub async fn get_checkpoint(
        &self,
        tag_name: &str,
        alarm_type: &str,
    ) -> Result<Option<QueryCheckpoint>> {
        let row = sqlx::query(
            "SELECT tag_name, alarm_type, last_end_time
             FROM query_checkpoint
             WHERE tag_name = ?1 AND alarm_type = ?2",
        )
        .bind(tag_name)
        .bind(alarm_type)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| QueryCheckpoint {
            tag_name: r.get(0),
            alarm_type: r.get(1),
            last_end_time: r.get(2),
        }))
    }

    /// Count pending uploads for a target.
    pub async fn pending_count(&self, target_db: &str) -> Result<u32> {
        let row = sqlx::query(
            "SELECT COUNT(*) FROM upload_log WHERE status IN ('pending','failed') AND target_db=?1",
        )
        .bind(target_db)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>(0) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn state_cannot_be_rebound_to_a_different_endpoint() {
        let db = LocalDb::open(Path::new(":memory:"), "source-a", Some("db-a"), false)
            .await
            .unwrap();
        assert!(db.migrate("source-a", Some("db-a"), false).await.is_ok());
        assert!(db.migrate("source-a", None, false).await.is_ok());
        assert!(db.migrate("source-b", Some("db-a"), false).await.is_err());
        assert!(db.migrate("source-a", Some("db-b"), false).await.is_err());
    }

    #[tokio::test]
    async fn adoption_preserves_completed_legacy_uploads() {
        let db = LocalDb::open(Path::new(":memory:"), "old-source", Some("old-db"), false)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sync_identity")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("DROP INDEX uniq_upload_log_alarm_target")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO alarms (tag_name, alarm_type, datetime)
                     VALUES ('sample', 'AdvAlm', 60)",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO upload_log (alarm_id, target_db, status)
                     VALUES (1, 'postgresql', 'pending'), (1, 'postgresql', 'uploaded')",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        assert!(
            db.migrate("old-source", Some("old-db"), false)
                .await
                .is_err()
        );
        assert!(db.migrate("old-source", None, true).await.is_err());
        db.migrate("old-source", Some("old-db"), true)
            .await
            .unwrap();
        let status: Vec<String> = sqlx::query_scalar("SELECT status FROM upload_log")
            .fetch_all(&db.pool)
            .await
            .unwrap();
        assert_eq!(status, ["uploaded"]);
    }

    #[tokio::test]
    async fn archive_and_checkpoint_commit_together() {
        let db = LocalDb::open(Path::new(":memory:"), "source-a", Some("db-a"), false)
            .await
            .unwrap();
        let record = AlarmRecord {
            id: None,
            tag_name: "sample".into(),
            alarm_type: "AdvAlm".into(),
            datetime: 60,
            mseconds: 0,
            value: 1,
            comment: String::new(),
        };
        sqlx::query(
            "CREATE TRIGGER test_queue_failure BEFORE INSERT ON upload_log
             BEGIN SELECT RAISE(ABORT, 'temporary queue failure'); END",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        assert!(
            db.commit_window(
                "sample",
                "AdvAlm",
                std::slice::from_ref(&record),
                Some("postgresql"),
                70
            )
            .await
            .is_err()
        );
        assert!(
            db.get_checkpoint("sample", "AdvAlm")
                .await
                .unwrap()
                .is_none()
        );
        let alarms: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alarms")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(alarms, 0);

        sqlx::query("DROP TRIGGER test_queue_failure")
            .execute(&db.pool)
            .await
            .unwrap();
        let records = &[record];
        assert_eq!(
            db.commit_window("sample", "AdvAlm", records, Some("postgresql"), 70)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            db.commit_window("sample", "AdvAlm", records, Some("postgresql"), 65)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            db.get_checkpoint("sample", "AdvAlm")
                .await
                .unwrap()
                .unwrap()
                .last_end_time,
            70
        );
        assert_eq!(db.pending_count("postgresql").await.unwrap(), 1);
    }
}
