use anyhow::{Context, Result};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
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
    pub async fn open(path: &Path) -> Result<Self> {
        let conn_str = path.to_str().context("Invalid SQLite path")?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(conn_str)
            .await
            .context("Failed to open SQLite database")?;

        // WAL mode + foreign keys
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await?;

        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
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
        .execute(&self.pool)
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
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_upload_log_status
             ON upload_log(status, target_db)",
        )
        .execute(&self.pool)
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
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Insert an alarm record. Returns the row id (existing or new).
    pub async fn insert_alarm(&self, record: &AlarmRecord) -> Result<i64> {
        sqlx::query(
            "INSERT OR IGNORE INTO alarms (tag_name, alarm_type, datetime, mseconds, value, comment)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&record.tag_name)
        .bind(&record.alarm_type)
        .bind(record.datetime)
        .bind(record.mseconds)
        .bind(record.value)
        .bind(&record.comment)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query(
            "SELECT id FROM alarms WHERE tag_name=?1 AND alarm_type=?2 AND datetime=?3 AND mseconds=?4",
        )
        .bind(&record.tag_name)
        .bind(&record.alarm_type)
        .bind(record.datetime)
        .bind(record.mseconds)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get(0))
    }

    /// Create an upload_log entry if it doesn't already exist.
    pub async fn ensure_upload_log(&self, alarm_id: i64, target_db: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO upload_log (alarm_id, target_db, status)
             VALUES (?1, ?2, 'pending')",
        )
        .bind(alarm_id)
        .bind(target_db)
        .execute(&self.pool)
        .await?;
        Ok(())
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
        .bind(limit as i64)
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

    /// Upsert a checkpoint.
    pub async fn save_checkpoint(
        &self,
        tag_name: &str,
        alarm_type: &str,
        last_end_time: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO query_checkpoint (tag_name, alarm_type, last_end_time, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(tag_name, alarm_type) DO UPDATE SET
                last_end_time = excluded.last_end_time,
                updated_at    = datetime('now')",
        )
        .bind(tag_name)
        .bind(alarm_type)
        .bind(last_end_time)
        .execute(&self.pool)
        .await?;
        Ok(())
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
