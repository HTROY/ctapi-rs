# alm-sync — Alarm Sync Demo

Sync Citect SCADA alarm data to local SQLite and remote PostgreSQL / MSSQL with breakpoint-resume support.

## Architecture

```
examples/alm-sync/
├── Cargo.toml
└── src/
    ├── main.rs        # CLI + main loop (tokio::main)
    ├── config.rs      # JSON config deserialization + CLI overrides
    ├── local_db.rs    # SQLite via sqlx: alarms, upload_log, query_checkpoint
    ├── alm_query.rs   # Wraps CtClient::find_first for AlmQuery
    └── uploader.rs    # Async upload to PostgreSQL (sqlx) or MSSQL (tiberius)
```

### Query cycle (`alm_query.rs`)

Builds `ALMQUERY,{type},{tag},{start},0,{end},0,{period}` command strings, calls `client.find_first()`, parses `DateTime` / `MSeconds` / `Comment` / `Value` properties. Runs via `spawn_blocking` since CtAPI is synchronous.

### Local SQLite (`local_db.rs`)

Three tables:
- **`alarms`** — alarm cache with dedup via `UNIQUE(tag_name, alarm_type, datetime, mseconds)`
- **`upload_log`** — per-alarm upload status (`pending` → `uploaded` / `failed` + retry_count)
- **`query_checkpoint`** — breakpoint resume per `(tag, alarm_type)`

### Uploader (`uploader.rs`)

Background tokio task polls `upload_log` for pending entries, batch-inserts to remote DB:
- **PostgreSQL**: `sqlx::PgPool` with batched `INSERT ... ON CONFLICT DO NOTHING`
- **MSSQL**: `tiberius` with `INSERT ... WHERE NOT EXISTS` per-record

### Breakpoint resume

On restart, reads `query_checkpoint` for each tag and queries from the saved `last_end_time` instead of the configured lookback.

## Usage

```bash
cargo run -p alm-sync -- --config config.json
cargo run -p alm-sync -- --config config.json --once
```

## Config File

```json
{
  "scada": {
    "computer": "127.0.0.1",
    "user": "alarm_reader"
  },
  "query": {
    "alarm_type": "AdvAlm",
    "tag_names": ["Feed_SPC_11"],
    "period": 0.001,
    "lookback_days": 80,
    "poll_interval_secs": 300
  },
  "sqlite": {
    "path": "alarms.db"
  },
  "upload": {
    "target": "postgresql",
    "connection_string": "postgresql://alarm_writer@db.example/alarms?sslmode=verify-full",
    "batch_size": 100,
    "retry_max": 3,
    "retry_interval_secs": 30
  }
}
```

For MSSQL, use `"target": "mssql"` with an ADO.NET connection string:

```json
"upload": {
  "target": "mssql",
  "connection_string": "Server=db.example,1433;Database=alarms;User ID=alarm_writer;Encrypt=true;TrustServerCertificate=false",
  "batch_size": 100,
  "retry_max": 3,
  "retry_interval_secs": 30
}
```

### CLI Options

| Flag | Description |
|------|-------------|
| `--config <path>` | Path to JSON config file (default: `config.json`) |
| `--computer <host>` | Override SCADA computer address |
| `--user <name>` | Override SCADA user |
| `--once` | Query once and exit (default: continuous polling) |

## Credentials and transport

Supply the SCADA password through `CITECT_PASSWORD` or a permission-restricted JSON file. The environment value takes precedence. Store database credentials in a restricted connection configuration or use the driver's supported credential provider. Use an account with read-only SCADA access. The client requests CtAPI encryption and read-only mode.

PostgreSQL uploads require TLS with full server identity verification. MSSQL uploads require encryption, server certificate verification, and an explicit database name. Use the hostname covered by the server certificate and configure its trusted CA.

## SQLite state

Set `sqlite.path` to a local filename. A new database is associated with the selected SCADA host and user, and with its upload destination if configured. Use a separate file when either identity changes. Temporarily disabling uploads keeps the existing destination binding.

For an existing database created by an older version, back up the file and check its source and queued destination against the current configuration. Run once with `--adopt-existing-state` to record that verified binding and reconcile duplicate upload entries. Use a new database if the origin is unclear.

An open cursor must enumerate its reported record count. Each completed query window commits its alarms, upload entries, and checkpoint in one transaction. Initial queries cover at most one day and are reduced on exceeding 10,000 records. If a one-second window still exceeds the limit, the sync reports an error and retains the previous checkpoint.

## Query Fields

| Field | Description |
|-------|-------------|
| `DateTime` | Alarm time in seconds since 1970 (UTC) |
| `MSeconds` | Millisecond component (0–999) |
| `Comment` | Latest user comment from SOE page |
| `Value` | Bit field: bGood(0), bDisabled(1), bMultiple(2), bOn(3), bAck(4), state(5–7) |

## Supported Alarm Types

`DigAlm`, `AnaAlm`, `AdvAlm`, `HResAlm`, `ArgDigAlm`, `TsDigAlm`, `TsAnaAlm`
