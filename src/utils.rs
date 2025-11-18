//! A collection of utility functions
use std::time::SystemTime;

use chrono::DateTime;
use chrono::Utc;

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

/// Formats a file size in bytes into a human-readable string (e.g., KB, MB).
///
/// # Arguments
/// * `size` - The file size in bytes.
///
/// # Returns
/// * `String` - The formatted file size.
pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}
