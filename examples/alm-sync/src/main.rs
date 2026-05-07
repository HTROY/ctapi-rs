//! AlmQuery Alarm Sync Demo
//!
//! Queries alarm data from Citect SCADA via AlmQuery, stores it in a local
//! SQLite database, and optionally uploads it to a remote PostgreSQL or MSSQL
//! database with breakpoint-resume support.
//!
//! ## Usage
//!
//! ```text
//! alm-sync --config config.json
//! alm-sync --config config.json --computer 192.168.1.100 --user admin --password secret
//! ```
//!
//! ## Config file example (config.json)
//!
//! ```json
//! {
//!   "scada": {
//!     "computer": "127.0.0.1",
//!     "user": "Engineer",
//!     "password": "Citect"
//!   },
//!   "query": {
//!     "alarm_type": "AdvAlm",
//!     "tag_names": ["Feed_SPC_11", "Tank_Level_3"],
//!     "period": 0.001,
//!     "lookback_days": 80,
//!     "poll_interval_secs": 300
//!   },
//!   "sqlite": {
//!     "path": "alarms.db"
//!   },
//!   "upload": {
//!     "target": "postgresql",
//!     "connection_string": "postgresql://user:pass@localhost/alarms",
//!     "batch_size": 100,
//!     "retry_max": 3,
//!     "retry_interval_secs": 30
//!   }
//! }
//! ```

mod alm_query;
mod config;
mod local_db;
mod uploader;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use config::Config;
use ctapi_rs::CtClient;
use local_db::LocalDb;
use log::{error, info, warn};
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

    /// Override SCADA password
    #[arg(long)]
    password: Option<String>,

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
    if let Some(ref password) = cli.password {
        config.scada.password = password.clone();
    }

    Ok(config)
}

/// Determine the start time for a tag query, respecting any saved checkpoint.
async fn resolve_start_time(
    db: &LocalDb,
    tag_name: &str,
    alarm_type: &str,
    default_lookback_days: i64,
) -> i64 {
    let fallback = Utc::now()
        .checked_sub_signed(chrono::Duration::days(default_lookback_days))
        .unwrap_or_else(|| Utc::now())
        .timestamp();

    match db.get_checkpoint(tag_name, alarm_type).await {
        Ok(Some(cp)) => {
            info!(
                "Resuming {}/{} from checkpoint: {}",
                tag_name, alarm_type, cp.last_end_time
            );
            cp.last_end_time
        }
        Ok(None) => fallback,
        Err(e) => {
            warn!(
                "Failed to read checkpoint for {}/{}: {}",
                tag_name, alarm_type, e
            );
            fallback
        }
    }
}

/// Run a single query cycle: fetch alarms for all configured tags, save to
/// SQLite, and update checkpoints.
async fn run_query_cycle(
    client: Arc<CtClient>,
    db: &LocalDb,
    query_cfg: &config::QueryConfig,
    upload_cfg: &Option<config::UploadConfig>,
) -> Result<usize> {
    let end_time = Utc::now().timestamp();
    let mut total_new = 0usize;
    let target_db = upload_cfg
        .as_ref()
        .map(|u| u.target.as_str())
        .unwrap_or("none");

    for tag_name in &query_cfg.tag_names {
        let start_time = resolve_start_time(
            db,
            tag_name,
            &query_cfg.alarm_type,
            query_cfg.lookback_days,
        )
        .await;

        if start_time >= end_time {
            info!("No new data to query for {}", tag_name);
            continue;
        }

        info!(
            "Querying {} ({}), range {}..{}",
            tag_name, query_cfg.alarm_type, start_time, end_time
        );

        // SCADA calls are synchronous — run them on a blocking thread
        let alarm_type = query_cfg.alarm_type.clone();
        let t = tag_name.clone();
        let period = query_cfg.period;
        let client_clone = Arc::clone(&client);

        let records = tokio::task::spawn_blocking(move || {
            alm_query::query_alarms(&client_clone, &alarm_type, &t, start_time, end_time, period)
        })
        .await
        .context("spawn_blocking panicked")??;

        let batch_count = records.len();
        for record in &records {
            match db.insert_alarm(record).await {
                Ok(alarm_id) => {
                    total_new += 1;
                    if upload_cfg.is_some() {
                        if let Err(e) = db.ensure_upload_log(alarm_id, target_db).await {
                            warn!("Failed to create upload log for alarm {}: {}", alarm_id, e);
                        }
                    }
                }
                Err(e) => warn!("Failed to insert alarm: {}", e),
            }
        }

        if let Err(e) = db.save_checkpoint(tag_name, &query_cfg.alarm_type, end_time).await {
            warn!("Failed to save checkpoint: {}", e);
        }

        info!(
            "{} — {} records returned, {} new inserted",
            tag_name, batch_count, total_new
        );
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

    // Open local SQLite
    let db = LocalDb::open(&PathBuf::from(&config.sqlite.path)).await?;
    info!("Local SQLite opened: {}", config.sqlite.path);

    // Connect to SCADA (sync, but CtClient is Arc-safe for sharing)
    let client = Arc::new(CtClient::open(
        Some(&config.scada.computer),
        Some(&config.scada.user),
        Some(&config.scada.password),
        0,
    )?);
    info!(
        "Connected to SCADA at {} as {}",
        config.scada.computer, config.scada.user
    );

    // Spawn upload task if configured
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let upload_handle = if let Some(ref upload_cfg) = config.upload {
        let db_clone = db.clone();
        let cfg_clone = upload_cfg.clone();
        Some(tokio::spawn(async move {
            uploader::upload_loop(db_clone, cfg_clone, shutdown_rx).await;
        }))
    } else {
        info!("No upload target configured — alarms saved to SQLite only");
        None
    };

    // --- Main loop ---
    loop {
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
                info!("Cycle complete: {} new alarms, {} pending upload", n, pending);
            }
            Err(e) => error!("Query cycle failed: {}", e),
        }

        if cli.once {
            info!("One-shot mode — exiting main loop");
            break;
        }

        info!(
            "Sleeping {}s until next poll...",
            config.query.poll_interval_secs
        );
        sleep(Duration::from_secs(config.query.poll_interval_secs)).await;
    }

    // Signal upload loop to stop and wait for it
    let _ = shutdown_tx.send(true);
    if let Some(handle) = upload_handle {
        let _ = handle.await;
    }

    info!("Alm-sync finished");
    Ok(())
}
