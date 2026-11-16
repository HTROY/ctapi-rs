use crate::config::UploadConfig;
use crate::local_db::{AlarmRecord, LocalDb};
use anyhow::{Context, Result, anyhow, ensure};
use connection_string::AdoNetString;
use log::{error, info, warn};
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;

type MssqlClient = tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>;

async fn ensure_remote_table_pg(pool: &sqlx::PgPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS alarms (
            id          BIGSERIAL PRIMARY KEY,
            tag_name    VARCHAR(256) NOT NULL,
            alarm_type  VARCHAR(64)  NOT NULL,
            datetime    BIGINT       NOT NULL,
            mseconds    INTEGER      NOT NULL DEFAULT 0,
            value       BIGINT       NOT NULL DEFAULT 0,
            comment     TEXT         NOT NULL DEFAULT '',
            synced_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
            UNIQUE(tag_name, alarm_type, datetime, mseconds)
        )",
    )
    .execute(pool)
    .await
    .context("Failed to create remote alarms table on PostgreSQL")?;
    Ok(())
}

async fn ensure_remote_table_mssql(client: &mut MssqlClient) -> Result<()> {
    let sql = "IF NOT EXISTS (SELECT * FROM sysobjects WHERE name='alarms' AND xtype='U')
               CREATE TABLE alarms (
                  id          BIGINT IDENTITY(1,1) PRIMARY KEY,
                  tag_name    NVARCHAR(256) NOT NULL,
                  alarm_type  NVARCHAR(64)  NOT NULL,
                  datetime    BIGINT        NOT NULL,
                  mseconds    INT           NOT NULL DEFAULT 0,
                  value       BIGINT        NOT NULL DEFAULT 0,
                  comment     NVARCHAR(MAX) NOT NULL DEFAULT '',
                  synced_at   DATETIME2     NOT NULL DEFAULT SYSUTCDATETIME(),
                  CONSTRAINT uq_alarm UNIQUE(tag_name, alarm_type, datetime, mseconds)
               )";
    client
        .execute(sql, &[])
        .await
        .context("Failed to create remote alarms table on MSSQL")?;
    Ok(())
}

async fn insert_batch_pg(pool: &sqlx::PgPool, records: &[AlarmRecord]) -> Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }

    let placeholders: Vec<String> = (0..records.len())
        .map(|i| {
            let base = i * 6;
            format!(
                "(${}, ${}, ${}, ${}, ${}, ${})",
                base + 1,
                base + 2,
                base + 3,
                base + 4,
                base + 5,
                base + 6,
            )
        })
        .collect();

    let sql = format!(
        "INSERT INTO alarms (tag_name, alarm_type, datetime, mseconds, value, comment)
         VALUES {}
         ON CONFLICT (tag_name, alarm_type, datetime, mseconds) DO NOTHING",
        placeholders.join(", ")
    );

    let mut query = sqlx::query(&sql);
    for r in records {
        query = query
            .bind(&r.tag_name)
            .bind(&r.alarm_type)
            .bind(r.datetime)
            .bind(r.mseconds)
            .bind(r.value)
            .bind(&r.comment);
    }

    let result = query.execute(pool).await?;
    Ok(result.rows_affected() as usize)
}

async fn insert_batch_mssql(client: &mut MssqlClient, records: &[AlarmRecord]) -> Result<usize> {
    use tiberius::ToSql;
    let mut count = 0usize;
    for r in records {
        let sql = "INSERT INTO alarms (tag_name, alarm_type, datetime, mseconds, value, comment)
                   SELECT @P1, @P2, @P3, @P4, @P5, @P6
                   WHERE NOT EXISTS (
                       SELECT 1 FROM alarms
                       WHERE tag_name=@P1 AND alarm_type=@P2 AND datetime=@P3 AND mseconds=@P4
                   )";
        let rows = client
            .execute(
                sql,
                &[
                    &r.tag_name.as_str() as &dyn ToSql,
                    &r.alarm_type.as_str() as &dyn ToSql,
                    &r.datetime as &dyn ToSql,
                    &r.mseconds as &dyn ToSql,
                    &r.value as &dyn ToSql,
                    &r.comment.as_str() as &dyn ToSql,
                ],
            )
            .await
            .context("MSSQL insert failed")?;
        count += rows.rows_affected().len();
    }
    Ok(count)
}

pub enum UploadConnection {
    Postgres(PgConnectOptions),
    Mssql(tiberius::Config),
}

impl UploadConnection {
    /// Resolve transport policy and cache identity from the configuration
    /// used by the actual connection. Identity fields exclude passwords.
    pub fn prepare(config: &UploadConfig) -> Result<(Self, String)> {
        match config.target.as_str() {
            "postgresql" => {
                let options = PgConnectOptions::from_str(&config.connection_string)
                    .map_err(|_| anyhow!("Invalid PostgreSQL connection string"))?
                    .ssl_mode(PgSslMode::VerifyFull);
                let database = options
                    .get_database()
                    .unwrap_or_else(|| options.get_username());
                let identity = identity_digest(&(
                    "postgresql",
                    options.get_host(),
                    options.get_port(),
                    options.get_socket(),
                    database,
                    options.get_username(),
                    options.get_options(),
                ))?;
                Ok((Self::Postgres(options), identity))
            }
            "mssql" => {
                // Match Tiberius's ADO.NET key and quoting rules, including
                // duplicate-key precedence and semicolons inside passwords.
                let fields = AdoNetString::from_str(&config.connection_string)
                    .map_err(|_| anyhow!("Invalid MSSQL ADO.NET connection string"))?;
                let skip_certificate = fields.get("trustservercertificate").is_some_and(|value| {
                    let value = value.trim();
                    value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
                });
                ensure!(
                    !skip_certificate,
                    "MSSQL requires server certificate validation"
                );
                let mut options = tiberius::Config::from_ado_string(&config.connection_string)
                    .map_err(|_| anyhow!("Invalid MSSQL ADO.NET connection options"))?;
                options.encryption(tiberius::EncryptionLevel::Required);
                let database = fields
                    .get("database")
                    .or_else(|| fields.get("initial catalog"))
                    .or_else(|| fields.get("databasename"))
                    .filter(|name| !name.is_empty())
                    .context("MSSQL requires an explicit database name to bind the sync state")?;
                let user = fields
                    .get("uid")
                    .or_else(|| fields.get("username"))
                    .or_else(|| fields.get("user"))
                    .or_else(|| fields.get("user id"));
                let identity = identity_digest(&("mssql", options.get_addr(), database, user))?;
                Ok((Self::Mssql(options), identity))
            }
            _ => Err(anyhow!("Upload target must be postgresql or mssql")),
        }
    }
}

fn identity_digest<T: serde::Serialize>(fields: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(fields)?)))
}

async fn build_mssql_client(config: &tiberius::Config) -> Result<MssqlClient> {
    use tokio_util::compat::TokioAsyncWriteCompatExt;
    let tcp = tokio::net::TcpStream::connect(config.get_addr())
        .await
        .context("Failed to connect to MSSQL server")?;
    tcp.set_nodelay(true)?;
    tiberius::Client::connect(config.clone(), tcp.compat_write())
        .await
        .context("Failed to create MSSQL client")
}

/// Background upload loop.
pub async fn upload_loop(
    db: LocalDb,
    config: UploadConfig,
    connection: UploadConnection,
    shutdown: tokio::sync::watch::Receiver<bool>,
) {
    info!(
        "Upload loop started, target={}, batch_size={}",
        config.target, config.batch_size
    );
    match connection {
        UploadConnection::Postgres(options) => upload_loop_pg(db, config, options, shutdown).await,
        UploadConnection::Mssql(options) => upload_loop_mssql(db, config, options, shutdown).await,
    }
    info!("Upload loop finished");
}

async fn upload_loop_pg(
    db: LocalDb,
    config: UploadConfig,
    connection: PgConnectOptions,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_with(connection)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to connect to PostgreSQL: {}", e);
            return;
        }
    };

    if let Err(e) = ensure_remote_table_pg(&pool).await {
        error!("Failed to ensure remote table: {}", e);
        return;
    }

    loop {
        if *shutdown.borrow() {
            break;
        }

        let pending = match db
            .get_pending_uploads(&config.target, config.batch_size, config.retry_max)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to read pending uploads: {}", e);
                sleep(Duration::from_secs(config.retry_interval_secs)).await;
                continue;
            }
        };

        if pending.is_empty() {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = sleep(Duration::from_secs(config.retry_interval_secs)) => continue,
            }
        }

        let records: Vec<AlarmRecord> = pending.iter().map(|e| e.record.clone()).collect();
        let log_ids: Vec<i64> = pending.iter().map(|e| e.id).collect();

        info!("Uploading {} alarm records to PostgreSQL", records.len());

        match insert_batch_pg(&pool, &records).await {
            Ok(n) => {
                info!("Uploaded {} records ({} sent)", n, records.len());
                if let Err(e) = db.mark_uploaded(&log_ids).await {
                    error!("Failed to mark records as uploaded: {}", e);
                }
            }
            Err(e) => {
                warn!("Batch upload failed: {}", e);
                if let Err(e2) = db.mark_failed(&log_ids, &e.to_string()).await {
                    error!("Failed to mark records as failed: {}", e2);
                }
                sleep(Duration::from_secs(config.retry_interval_secs)).await;
            }
        }
    }

    pool.close().await;
}

async fn upload_loop_mssql(
    db: LocalDb,
    config: UploadConfig,
    connection: tiberius::Config,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut client = match build_mssql_client(&connection).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to connect to MSSQL: {}", e);
            return;
        }
    };

    if let Err(e) = ensure_remote_table_mssql(&mut client).await {
        error!("Failed to ensure remote table on MSSQL: {}", e);
        return;
    }

    loop {
        if *shutdown.borrow() {
            break;
        }

        let pending = match db
            .get_pending_uploads(&config.target, config.batch_size, config.retry_max)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to read pending uploads: {}", e);
                sleep(Duration::from_secs(config.retry_interval_secs)).await;
                continue;
            }
        };

        if pending.is_empty() {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = sleep(Duration::from_secs(config.retry_interval_secs)) => continue,
            }
        }

        let records: Vec<AlarmRecord> = pending.iter().map(|e| e.record.clone()).collect();
        let log_ids: Vec<i64> = pending.iter().map(|e| e.id).collect();

        info!("Uploading {} alarm records to MSSQL", records.len());

        match insert_batch_mssql(&mut client, &records).await {
            Ok(n) => {
                info!("Uploaded {} records ({} sent)", n, records.len());
                if let Err(e) = db.mark_uploaded(&log_ids).await {
                    error!("Failed to mark records as uploaded: {}", e);
                }
            }
            Err(e) => {
                warn!("Batch upload failed: {}", e);
                if let Err(e2) = db.mark_failed(&log_ids, &e.to_string()).await {
                    error!("Failed to mark records as failed: {}", e2);
                }
                match build_mssql_client(&connection).await {
                    Ok(c) => client = c,
                    Err(e2) => error!("Failed to reconnect to MSSQL: {}", e2),
                }
                sleep(Duration::from_secs(config.retry_interval_secs)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(target: &str, connection_string: &str) -> UploadConfig {
        UploadConfig {
            target: target.to_owned(),
            connection_string: connection_string.to_owned(),
            batch_size: 100,
            retry_max: 3,
            retry_interval_secs: 30,
        }
    }

    #[test]
    fn postgres_requires_peer_verification_and_tracks_database() {
        let build = |database: &str, password: &str| {
            UploadConnection::prepare(&config(
                "postgresql",
                &format!("postgresql://tester:{password}@db.example/{database}?sslmode=disable"),
            ))
            .unwrap()
        };
        let (connection, identity) = build("alarms", "fixture");
        let UploadConnection::Postgres(options) = connection else {
            panic!("unexpected upload backend");
        };
        assert!(matches!(options.get_ssl_mode(), PgSslMode::VerifyFull));
        assert_eq!(identity, build("alarms", "rotated").1);
        assert_ne!(identity, build("archive", "fixture").1);
    }

    #[test]
    fn mssql_accepts_quoted_values_and_requires_certificate_validation() {
        let build = |certificate: &str| {
            UploadConnection::prepare(&config(
                "mssql",
                &format!(
                    "Server=db.example;Database=alarms;UID=tester;PWD='a;b';TrustServerCertificate={certificate}"
                ),
            ))
        };
        assert!(build("false").is_ok());
        assert!(build(" YeS ").is_err());
    }
}
