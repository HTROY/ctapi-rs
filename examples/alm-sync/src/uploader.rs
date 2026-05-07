use crate::config::UploadConfig;
use crate::local_db::{AlarmRecord, LocalDb};
use anyhow::{Context, Result};
use log::{error, info, warn};
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

async fn insert_batch_mssql(
    client: &mut MssqlClient,
    records: &[AlarmRecord],
) -> Result<usize> {
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

async fn build_pg_pool(config: &UploadConfig) -> Result<sqlx::PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.connection_string)
        .await
        .context("Failed to connect to PostgreSQL")?;
    Ok(pool)
}

async fn build_mssql_client(config: &UploadConfig) -> Result<MssqlClient> {
    use tokio_util::compat::TokioAsyncWriteCompatExt;
    let tcfg = tiberius::Config::from_ado_string(&config.connection_string)
        .context("Failed to parse MSSQL ADO.NET connection string")?;
    let tcp = tokio::net::TcpStream::connect(tcfg.get_addr())
        .await
        .context("Failed to connect to MSSQL server")?;
    tcp.set_nodelay(true)?;
    let compat = tcp.compat_write();
    let client = tiberius::Client::connect(tcfg, compat)
        .await
        .context("Failed to create MSSQL client")?;
    Ok(client)
}

/// Background upload loop.
pub async fn upload_loop(
    db: LocalDb,
    config: UploadConfig,
    shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let target = config.target.clone();
    info!(
        "Upload loop started, target={}, batch_size={}",
        target, config.batch_size
    );

    match target.as_str() {
        "postgresql" => upload_loop_pg(db, config, shutdown).await,
        "mssql" => upload_loop_mssql(db, config, shutdown).await,
        _ => {
            error!("Unsupported upload target: {}", target);
        }
    }

    info!("Upload loop finished");
}

async fn upload_loop_pg(
    db: LocalDb,
    config: UploadConfig,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let pool = match build_pg_pool(&config).await {
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
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut client = match build_mssql_client(&config).await {
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
                match build_mssql_client(&config).await {
                    Ok(c) => client = c,
                    Err(e2) => error!("Failed to reconnect to MSSQL: {}", e2),
                }
                sleep(Duration::from_secs(config.retry_interval_secs)).await;
            }
        }
    }
}
