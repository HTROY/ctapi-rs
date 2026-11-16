//! AlmQuery Alarm Sync Demo
//!
//! Queries alarm data from Citect SCADA via AlmQuery, stores it in a local
//! SQLite database, and optionally uploads it to a remote PostgreSQL or MSSQL
//! database with breakpoint-resume support.
//!
//! See the package README for setup, credentials and TLS configuration.

mod alm_query;
mod config;
mod local_db;
mod uploader;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::Parser;
use config::Config;
use ctapi_rs::{CT_OPEN_CRYPT, CT_OPEN_READ_ONLY, CtClient};
use local_db::LocalDb;
use log::{error, info};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Sync Citect SCADA alarms to local SQLite and remote PostgreSQL/MSSQL.
#[derive(Parser, Debug)]
#[command(name = "alm-sync", version)]
struct Cli {
    /// Path to JSON config file
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,

    /// Override SCADA computer address
    #[arg(long)]
    computer: Option<String>,

    /// Override SCADA user
    #[arg(long)]
    user: Option<String>,

    /// Associate a legacy SQLite file with its verified SCADA and upload configuration.
    #[arg(long)]
    adopt_existing_state: bool,

    /// One-shot mode: query once and exit (default is continuous polling)
    #[arg(long)]
    once: bool,
}

fn load_config(cli: &Cli) -> Result<Config> {
    let file = std::fs::File::open(&cli.config)
        .with_context(|| format!("Failed to open config file: {}", cli.config.display()))?;
    let mut config: Config =
        serde_json::from_reader(file).context("Failed to parse config file")?;

    if let Some(ref computer) = cli.computer {
        config.scada.computer = computer.clone();
    }
    if let Some(ref user) = cli.user {
        config.scada.user = user.clone();
    }
    match std::env::var("CITECT_PASSWORD") {
        Ok(password) => config.scada.password = password,
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(_)) => bail!("CITECT_PASSWORD must be valid Unicode"),
    }

    Ok(config)
}

/// Run a query cycle in bounded windows, committing complete windows in order.
async fn run_query_cycle(
    client: Arc<CtClient>,
    db: &LocalDb,
    query_cfg: &config::QueryConfig,
    upload_cfg: &Option<config::UploadConfig>,
) -> Result<usize> {
    let now = Utc::now();
    let end_time = now.timestamp();
    let lookback = chrono::Duration::try_days(query_cfg.lookback_days)
        .filter(|duration| *duration >= chrono::Duration::zero())
        .context("Alarm lookback must be a nonnegative duration")?;
    let first_start = now
        .checked_sub_signed(lookback)
        .context("Alarm lookback date is out of range")?
        .timestamp();
    let target_db = upload_cfg.as_ref().map(|u| u.target.as_str());
    let mut total_new = 0;

    for tag_name in &query_cfg.tag_names {
        let mut start_time = db
            .get_checkpoint(tag_name, &query_cfg.alarm_type)
            .await?
            .map_or(first_start, |checkpoint| checkpoint.last_end_time);

        while start_time < end_time {
            let mut window_secs = end_time
                .saturating_sub(start_time)
                .min(alm_query::MAX_WINDOW_SECS);
            let (records, window_end) = loop {
                let window_end = start_time + window_secs;
                let alarm_type = query_cfg.alarm_type.clone();
                let tag = tag_name.clone();
                let period = query_cfg.period;
                let client = Arc::clone(&client);
                let batch = tokio::task::spawn_blocking(move || {
                    alm_query::query_alarms(
                        &client,
                        &alarm_type,
                        &tag,
                        start_time,
                        window_end,
                        period,
                    )
                })
                .await
                .context("Alarm query worker failed")??;

                match batch {
                    alm_query::QueryBatch::Complete(records) => break (records, window_end),
                    alm_query::QueryBatch::LimitReached if window_secs > 1 => window_secs /= 2,
                    alm_query::QueryBatch::LimitReached => {
                        bail!("One second of alarm data exceeds the record budget")
                    }
                }
            };

            total_new += db
                .commit_window(
                    tag_name,
                    &query_cfg.alarm_type,
                    &records,
                    target_db,
                    window_end,
                )
                .await?;
            start_time = window_end;
        }
    }

    Ok(total_new)
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();
    let config = load_config(&cli)?;

    let upload = config
        .upload
        .as_ref()
        .map(uploader::UploadConnection::prepare)
        .transpose()?;
    let source = serde_json::to_string(&(&config.scada.computer, &config.scada.user))?;
    let db = LocalDb::open(
        &PathBuf::from(&config.sqlite.path),
        &source,
        upload.as_ref().map(|(_, destination)| destination.as_str()),
        cli.adopt_existing_state,
    )
    .await?;
    info!("Local SQLite opened: {}", config.sqlite.path);

    let client = Arc::new(CtClient::open(
        Some(&config.scada.computer),
        Some(&config.scada.user),
        Some(&config.scada.password),
        CT_OPEN_CRYPT | CT_OPEN_READ_ONLY,
    )?);
    info!(
        "Connected to SCADA at {} as {}",
        config.scada.computer, config.scada.user
    );

    // Spawn upload task if configured
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let upload_handle = if let (Some(upload_cfg), Some((connection, _))) = (&config.upload, upload)
    {
        let db_clone = db.clone();
        let cfg_clone = upload_cfg.clone();
        Some(tokio::spawn(async move {
            uploader::upload_loop(db_clone, cfg_clone, connection, shutdown_rx).await;
        }))
    } else {
        info!("No upload target configured — alarms saved to SQLite only");
        None
    };

    let query_result = loop {
        match run_query_cycle(Arc::clone(&client), &db, &config.query, &config.upload).await {
            Ok(n) => {
                let pending = db
                    .pending_count(
                        config
                            .upload
                            .as_ref()
                            .map(|u| u.target.as_str())
                            .unwrap_or("none"),
                    )
                    .await
                    .unwrap_or(0);
                info!(
                    "Cycle complete: {} new alarms, {} pending upload",
                    n, pending
                );
            }
            Err(error) if cli.once => break Err(error.context("Query cycle failed")),
            Err(error) => error!("Query cycle failed: {error}"),
        }

        if cli.once {
            info!("One-shot mode — exiting main loop");
            break Ok(());
        }

        info!(
            "Sleeping {}s until next poll...",
            config.query.poll_interval_secs
        );
        sleep(Duration::from_secs(config.query.poll_interval_secs)).await;
    };

    // Signal upload loop to stop and wait for it
    let _ = shutdown_tx.send(true);
    if let Some(handle) = upload_handle {
        let _ = handle.await;
    }

    info!("Alm-sync finished");
    query_result
}
