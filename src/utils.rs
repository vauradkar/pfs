//! A collection of utility functions
use std::time::SystemTime;

use chrono::{DateTime, Utc};

use crate::errors::Error;

/// Formats a `SystemTime` into a RFC 3339 - Z format.
/// For example "2018-01-26T18:30:09.453Z"
pub fn format_system_time(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Builds a `SystemTime` from a RFC 3339 - Z formatted string.
/// For example "2018-01-26T18:30:09.453Z"
pub fn parse_system_time(s: &str) -> Result<SystemTime, Error> {
    let datetime = DateTime::parse_from_rfc3339(s).map_err(|e| Error::Parse {
        what: "parse system time".into(),
        how: e.to_string(),
    })?;
    Ok(SystemTime::from(datetime))
}
