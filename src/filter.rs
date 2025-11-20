use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

use derivative::Derivative;
#[cfg(feature = "json_schema")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::Error;

/// A struct to configure and enforce path filtering rules.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Derivative, PartialEq, Eq)]
/// Enumertates the type of operations allowed/denied on a path
pub enum FilterLevel {
    /// Deny traversing and returning path
    Deny,

    /// Allow traversing but deny returning path
    /// This is applicable to directories
    Traverse,

    /// Allow both traversing and returning path
    Allow,
}

/// A struct to configure and enforce path filtering rules.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Derivative, PartialEq, Eq, Default)]
pub struct FilterSet {
    /// Paths that are explicitly allowed.
    /// If empty, all paths are allowed unless denied.
    allowed_roots: Vec<PathBuf>,

    /// Paths that are explicitly denied (blacklist mode).
    /// These override allowed_roots.
    denied_roots: Vec<PathBuf>,

    /// Allowed file extensions (e.g., "jpg", "png").
    /// If empty, all extensions are allowed.
    allowed_extensions: HashSet<String>,

    /// Allowed specific file names (e.g., "README.md").
    /// If empty, checking is skipped.
    allowed_filenames: HashSet<String>,
}

impl FilterSet {
    /// Create a new, empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new fitler with given filters
    #[allow(dead_code)]
    pub fn create_with<P: AsRef<Path>, S: AsRef<str>>(
        allowed_paths: &[P],
        denied_paths: &[P],
        allowed_filenames: &[S],
        allowed_extensions: &[S],
    ) -> Self {
        Self {
            allowed_roots: allowed_paths
                .iter()
                .map(|p| p.as_ref().to_path_buf())
                .collect(),
            denied_roots: denied_paths
                .iter()
                .map(|p| p.as_ref().to_path_buf())
                .collect(),
            allowed_filenames: allowed_filenames
                .iter()
                .map(|f| f.as_ref().into())
                .collect(),
            allowed_extensions: allowed_extensions
                .iter()
                .map(|e| e.as_ref().to_lowercase())
                .collect(),
        }
    }

    pub fn allow_path<P: AsRef<Path>>(&mut self, path: P) {
        self.allowed_roots.push(path.as_ref().to_path_buf());
    }

    pub fn deny_path<P: AsRef<Path>>(&mut self, path: P) {
        self.denied_roots.push(path.as_ref().to_path_buf());
    }

    pub fn allow_extension(&mut self, ext: &str) {
        self.allowed_extensions.insert(ext.to_lowercase());
    }

    pub fn allow_filename(&mut self, name: &str) {
        self.allowed_filenames.insert(name.to_string());
    }

    /// Determines if a path matches the filter criteria.
    ///
    /// Returns `true` if the path passes all checks.
    pub fn matches<P: AsRef<Path>>(&self, path: P, is_dir: bool) -> Result<FilterLevel, Error> {
        let path = path.as_ref();

        // Check Deny List
        // If the path starts with any denied root, it is rejected.
        for denied in &self.denied_roots {
            if path.starts_with(denied) {
                return Ok(FilterLevel::Deny);
            }
        }

        // Check Allow List
        // If we have allowed roots, the path MUST start with one of them.
        if !self.allowed_roots.is_empty() {
            let matches_allow = self.allowed_roots.iter().any(|root| path.starts_with(root));
            if !matches_allow {
                return Ok(FilterLevel::Deny);
            }
        }

        if is_dir && self.allowed_extensions.is_empty() && self.allowed_filenames.is_empty() {
            return Ok(FilterLevel::Allow);
        } else if is_dir {
            // There might be more files under the dir that might match filter
            // criteria
            return Ok(FilterLevel::Traverse);
        }

        // File-specific checks (Extension and Filename)
        // Only apply these checks if the path doesn't look like a directory
        if !self.allowed_extensions.is_empty() {
            if let Some(ext) = path.extension() {
                if !self.check_extension(ext) {
                    return Ok(FilterLevel::Deny);
                }
            } else {
                return Ok(FilterLevel::Deny);
            }
        }

        // Check Filename specifically (if configured)
        if !self.allowed_filenames.is_empty() && !self.check_filename(path) {
            return Ok(FilterLevel::Deny);
        }

        Ok(FilterLevel::Allow)
    }

    fn check_extension(&self, ext: &OsStr) -> bool {
        if let Some(ext_str) = ext.to_str() {
            return self.allowed_extensions.contains(&ext_str.to_lowercase());
        }
        false
    }

    fn check_filename(&self, path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            return self.allowed_filenames.contains(name);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path as StdPath;

    use super::*;

    #[test]
    fn test_filter_it_with_path() {
        let fset = FilterSet::create_with::<&str, &str>(&[], &[], &[], &["rs"]);

        // Test with different path types
        assert_eq!(fset.matches("main.rs", false).unwrap(), FilterLevel::Allow);
        assert_eq!(
            fset.matches("main_rs", true).unwrap(),
            FilterLevel::Traverse
        );
        assert_eq!(
            fset.matches(String::from("test.rs"), false).unwrap(),
            FilterLevel::Allow
        );
        assert_eq!(
            fset.matches(PathBuf::from("src/lib.rs"), false).unwrap(),
            FilterLevel::Allow
        );
        assert_eq!(
            fset.matches(StdPath::new("module.rs"), false).unwrap(),
            FilterLevel::Allow
        );

        assert_eq!(fset.matches("main.txt", false).unwrap(), FilterLevel::Deny);
    }

    #[test]
    fn test_filter_it_deny() {
        let filterset = FilterSet::create_with::<&str, &str>(&[], &["target"], &[], &[]);

        assert_eq!(
            filterset.matches("target/debug/main", true).unwrap(),
            FilterLevel::Deny
        );
        assert_eq!(
            filterset.matches("main/debug/target", true).unwrap(),
            FilterLevel::Allow
        );
        assert_eq!(
            filterset.matches("main/target/debug", true).unwrap(),
            FilterLevel::Allow
        );
        assert_eq!(
            filterset.matches("src/main.rs", true).unwrap(),
            FilterLevel::Allow
        );
    }

    #[test]
    fn test_filter_it_allow_and_deny() {
        let filterset = FilterSet::create_with::<&str, &str>(&[], &[], &[], &["rs"]);

        assert_eq!(
            filterset.matches("main.rs", false).unwrap(),
            FilterLevel::Allow
        );
        assert_eq!(
            filterset.matches("test_main.rs", false).unwrap(),
            FilterLevel::Allow
        );
        assert_eq!(
            filterset.matches("main.txt", false).unwrap(),
            FilterLevel::Deny
        );
    }

    #[test]
    fn test_filter_deny_overrides_allow() {
        let filterset =
            FilterSet::create_with::<&str, &str>(&["target/debug"], &["target"], &[], &["rs"]);

        assert_eq!(
            filterset.matches("target", false).unwrap(),
            FilterLevel::Deny
        );
        assert_eq!(
            filterset.matches("target", true).unwrap(),
            FilterLevel::Deny
        );
        assert_eq!(
            filterset.matches("target/debug", true).unwrap(),
            FilterLevel::Deny
        );
        assert_eq!(
            filterset.matches("target/debug", false).unwrap(),
            FilterLevel::Deny
        );
        assert_eq!(
            filterset.matches("target/debug/test.rs", false).unwrap(),
            FilterLevel::Deny
        );
    }
}
