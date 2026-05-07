use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub scada: ScadaConfig,
    pub query: QueryConfig,
    pub sqlite: SqliteConfig,
    pub upload: Option<UploadConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScadaConfig {
    pub computer: String,
    pub user: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryConfig {
    pub alarm_type: String,
    #[serde(default)]
    pub tag_names: Vec<String>,
    #[serde(default = "default_period")]
    pub period: f64,
    #[serde(default = "default_lookback_days")]
    pub lookback_days: i64,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            alarm_type: "AdvAlm".into(),
            tag_names: vec![],
            period: default_period(),
            lookback_days: default_lookback_days(),
            poll_interval_secs: default_poll_interval_secs(),
        }
    }
}

fn default_period() -> f64 { 0.001 }
fn default_lookback_days() -> i64 { 80 }
fn default_poll_interval_secs() -> u64 { 300 }

#[derive(Debug, Clone, Deserialize)]
pub struct SqliteConfig {
    #[serde(default = "default_sqlite_path")]
    pub path: String,
}

fn default_sqlite_path() -> String { "alarms.db".into() }

#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    /// "postgresql" or "mssql"
    pub target: String,
    pub connection_string: String,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_retry_max")]
    pub retry_max: u32,
    #[serde(default = "default_retry_interval_secs")]
    pub retry_interval_secs: u64,
}

fn default_batch_size() -> usize { 100 }
fn default_retry_max() -> u32 { 3 }
fn default_retry_interval_secs() -> u64 { 30 }
