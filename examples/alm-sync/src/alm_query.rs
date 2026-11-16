use crate::local_db::AlarmRecord;
use anyhow::{Context, Result, ensure};
use ctapi_rs::CtClient;
use log::debug;

pub const MAX_WINDOW_SECS: i64 = 24 * 60 * 60;
const MAX_RECORDS: usize = 10_000;

pub enum QueryBatch {
    Complete(Vec<AlarmRecord>),
    LimitReached,
}

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
) -> Result<QueryBatch> {
    ensure!(
        period.is_finite() && period > 0.0,
        "Alarm sample period must be positive and finite"
    );
    for (input, field) in [(alarm_type, "type"), (tag_name, "tag")] {
        ensure!(
            !input.is_empty() && !input.contains([',', '\0']),
            "Alarm query {field} must be a single nonempty field"
        );
    }
    ensure!(
        (1..=MAX_WINDOW_SECS).contains(&end_time.saturating_sub(start_time)),
        "Alarm query window is outside the allowed range"
    );
    let cmd = format!(
        "ALMQUERY,{},{},{},0,{},0,{}",
        alarm_type, tag_name, start_time, end_time, period
    );
    let mut find = client.try_find_first(&cmd, None, None)?;
    let mut next = find.try_next()?;
    if next.is_none() {
        return Ok(QueryBatch::Complete(Vec::new()));
    }

    // Both bundled DLLs can report end-of-search after a failed page fetch.
    // Compare the count for the open cursor with the complete traversal.
    let expected = find.record_count()?;
    if expected > MAX_RECORDS {
        return Ok(QueryBatch::LimitReached);
    }

    let mut records = Vec::with_capacity(expected);
    let mut seen = 0;
    while let Some(object) = next {
        seen += 1;
        ensure!(
            seen <= expected,
            "CtAPI returned more rows than the search count"
        );
        let datetime: i64 = object
            .get_property("DateTime")
            .context("Failed to read DateTime property")?
            .parse()
            .context("Failed to parse DateTime")?;

        // AlmQuery can include adjacent samples outside the requested range.
        if (start_time..end_time).contains(&datetime) {
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
        next = find.try_next()?;
    }

    ensure!(
        seen == expected,
        "CtAPI search ended after {seen} of {expected} rows"
    );

    debug!(
        "AlmQuery returned {} records for tag={} type={}",
        records.len(),
        tag_name,
        alarm_type
    );
    Ok(QueryBatch::Complete(records))
}
