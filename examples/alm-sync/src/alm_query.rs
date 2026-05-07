use crate::local_db::AlarmRecord;
use anyhow::{Context, Result};
use ctapi_rs::CtClient;
use log::{debug, info};

/// Run an AlmQuery against the SCADA system and return parsed alarm records.
///
/// Builds a command string of the form:
/// `ALMQUERY,{alarm_type},{tag_name},{start_secs},{start_ms},{end_secs},{end_ms},{period}`
///
/// See the AlmQuery documentation for details on the returned properties.
pub fn query_alarms(
    client: &CtClient,
    alarm_type: &str,
    tag_name: &str,
    start_time: i64,
    end_time: i64,
    period: f64,
) -> Result<Vec<AlarmRecord>> {
    let cmd = format!(
        "ALMQUERY,{},{},{},0,{},0,{}",
        alarm_type, tag_name, start_time, end_time, period
    );
    info!("AlmQuery: {}", cmd);

    let find = client.find_first(&cmd, "", None);
    let mut records = Vec::new();

    for object in find {
        let datetime: i64 = object
            .get_property("DateTime")
            .context("Failed to read DateTime property")?
            .parse()
            .context("Failed to parse DateTime")?;

        let mseconds: i32 = object
            .get_property("MSeconds")
            .context("Failed to read MSeconds property")?
            .parse()
            .context("Failed to parse MSeconds")?;

        let value: i64 = object
            .get_property("Value")
            .context("Failed to read Value property")?
            .parse()
            .context("Failed to parse Value")?;

        let comment = object
            .get_property("Comment")
            .context("Failed to read Comment property")?;

        records.push(AlarmRecord {
            id: None,
            tag_name: tag_name.to_string(),
            alarm_type: alarm_type.to_string(),
            datetime,
            mseconds,
            value,
            comment,
        });
    }

    debug!(
        "AlmQuery returned {} records for tag={} type={}",
        records.len(),
        tag_name,
        alarm_type
    );
    Ok(records)
}
