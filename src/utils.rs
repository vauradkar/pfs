//! A collection of utility functions
use std::path::Path as StdPath;
use std::time::SystemTime;

use chrono::DateTime;
use chrono::Utc;

use crate::errors::Error;

// Reserved names on Windows (case-insensitive)
static WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

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

/// Sanitizes a filename to be valid for Windows, macOS, and Linux platforms.
/// The filename need to be valid *across* these platforms.
///
/// # Arguments
/// * `filename` - The original filename to validate/sanitize
/// * `replacement` - Character to replace invalid characters with
///
/// # Returns
/// A sanitized filename that's valid on given platform
///
/// # Example
/// ```
/// let sanitized = pfs::utils::sanitize_filename("my<file>name?.txt", '_');
/// assert_eq!(sanitized, "my_file_name_.txt");
/// ```
pub fn sanitize_filename(filename: &str, replacement: char) -> String {
    // Characters invalid on Windows (which are the most restrictive)
    // < > : " / \ | ? *
    const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    // Control characters (0-31) are also invalid
    let is_invalid_char = |c: char| -> bool { INVALID_CHARS.contains(&c) || c.is_control() };

    // Replace invalid characters and coalesce consecutive replacements
    let mut sanitized = String::new();
    let mut last_was_replacement = false;

    for c in filename.chars() {
        if is_invalid_char(c) {
            if !last_was_replacement {
                sanitized.push(replacement);
                last_was_replacement = true;
            }
            // Skip adding more replacements if we just added one
        } else {
            sanitized.push(c);
            last_was_replacement = false;
        }
    }

    // Trim leading/trailing spaces and dots (invalid on Windows)
    sanitized = sanitized.trim_matches(|c| c == ' ' || c == '.').to_string();

    // Handle empty filename after sanitization
    if sanitized.is_empty() {
        return format!("unnamed{}", replacement);
    }

    let cloned_str = sanitized.clone();
    let mut path = StdPath::new(&cloned_str);
    let mut stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&cloned_str);

    // Check if the base name (without extension) is a Windows reserved name
    if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|&reserved| stem.eq_ignore_ascii_case(reserved))
    {
        sanitized = format!("{}{}{}", replacement, sanitized, replacement);
        path = StdPath::new(&sanitized);
        stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&sanitized);
    }

    // Ensure filename isn't too long (255 bytes is a common limit)
    if sanitized.len() > 255 {
        // Try to preserve the extension
        if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            let max_stem_len = 255 - extension.len() - 1; // -1 for the dot
            let truncated_stem = stem.chars().take(max_stem_len).collect::<String>();
            sanitized = format!("{}.{}", truncated_stem, extension);
        } else {
            sanitized = sanitized.chars().take(255).collect();
        }
    }

    sanitized
}

/// Checks if a filename is valid across all major platforms
///
/// # Arguments
/// * `filename` - The filename to validate
///
/// # Returns
/// `true` if the filename is valid on Windows, macOS, and Linux, `false`
/// otherwise
pub fn is_valid_filename(filename: &str) -> bool {
    const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    // Check if empty or too long
    if filename.is_empty() || filename.len() > 255 {
        return false;
    }

    // Check for invalid characters or control characters
    if filename
        .chars()
        .any(|c| INVALID_CHARS.contains(&c) || c.is_control())
    {
        return false;
    }

    // Check for leading/trailing spaces or dots
    if filename.starts_with(' ')
        || filename.starts_with('.')
        || filename.ends_with(' ')
        || filename.ends_with('.')
    {
        return false;
    }

    // Check for Windows reserved names
    let path = StdPath::new(filename);

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|&reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_filename() {
        assert!(is_valid_filename("myfile.txt"));
        assert!(is_valid_filename("my_file-123.txt"));
        assert!(is_valid_filename("file (1).txt"));
    }

    #[test]
    fn test_invalid_chars() {
        assert!(!is_valid_filename("my<file>.txt"));
        assert!(!is_valid_filename("file:name.txt"));
        assert!(!is_valid_filename("file/name.txt"));
    }

    #[test]
    fn test_reserved_names() {
        assert!(!is_valid_filename("CON"));
        assert!(!is_valid_filename("con.txt"));
        assert!(!is_valid_filename("PRN.doc"));
        assert!(!is_valid_filename("COM1"));
    }

    #[test]
    fn test_edge_cases() {
        assert!(!is_valid_filename(""));
        assert!(!is_valid_filename(" file.txt"));
        assert!(!is_valid_filename("file.txt "));
        assert!(!is_valid_filename(".file.txt"));
        assert!(!is_valid_filename("file.txt."));
    }

    #[test]
    fn test_sanitize() {
        assert_eq!(
            sanitize_filename("my<file>name.txt", '_'),
            "my_file_name.txt"
        );
        assert_eq!(sanitize_filename("file:name?.txt", '-'), "file-name-.txt");
        assert_eq!(
            sanitize_filename("path/to/file.txt", '_'),
            "path_to_file.txt"
        );
        assert_eq!(sanitize_filename("  .file.txt.  ", '_'), "file.txt");
    }

    #[test]
    fn test_coalesce_invalid_chars() {
        assert_eq!(
            sanitize_filename("my<////file>>name.txt", '_'),
            "my_file_name.txt"
        );
        assert_eq!(sanitize_filename("file:::<>name.txt", '-'), "file-name.txt");
        assert_eq!(sanitize_filename(">>>file<<<.txt", '_'), "_file_.txt");
        assert_eq!(sanitize_filename("a??**||b.txt", '-'), "a-b.txt");
    }

    #[test]
    fn test_sanitize_reserved_names() {
        let con_expected = "_CON.txt_";
        let prn_expected = "_prn_";
        assert_eq!(sanitize_filename("CON.txt", '_'), con_expected);
        assert_eq!(sanitize_filename("prn", '_'), prn_expected);
    }

    #[test]
    fn test_sanitize_empty() {
        assert_eq!(sanitize_filename("", '_'), "unnamed_");
        assert_eq!(sanitize_filename("...", '_'), "unnamed_");
        assert_eq!(sanitize_filename("   ", '_'), "unnamed_");
    }
}
