use std::fmt::Display;
use std::path::Path as StdPath;
use std::path::PathBuf;

#[cfg(feature = "poem")]
use poem_openapi::Object;
#[cfg(feature = "json_schema")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::de;

#[cfg(not(target_arch = "wasm32"))]
use crate::FileStat;
use crate::errors::Error;
#[cfg(not(target_arch = "wasm32"))]
use crate::file::FileInfo;

/// A custom deserializer function for a Vec<String> that checks for ".."
/// components.
fn deserialize_components<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let components = Vec::<String>::deserialize(deserializer)?;

    if components.iter().any(|c| c == ".." || c == ".") {
        // If an invalid component is found, return a custom error
        Err(de::Error::custom("Path component cannot contain '..'"))
    } else {
        // If all components are valid, return the result
        Ok(components)
    }
}

/// Represents a filesystem path as a vector of its portable components.
/// `Path` in itself is useless. It is a base/root path to be useful.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct Path {
    /// The components of the portable path as a vector of strings.
    #[serde(deserialize_with = "deserialize_components")]
    components: Vec<String>,
}

impl Display for Path {
    /// Format the portable `Path` for display by converting it into a
    /// platform `PathBuf` and delegating to its display implementation.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in 0..self.components.len().saturating_sub(1) {
            write!(f, "{}{}", self.components[i], std::path::MAIN_SEPARATOR_STR)?;
        }
        if !self.components.is_empty() {
            write!(f, "{}", self.components.last().unwrap())?;
        }
        Ok(())
    }
}

impl Path {
    /// Creates empty path
    pub fn empty() -> Self {
        Self { components: vec![] }
    }

    /// Returns the last component of the portable path, typically the file or
    /// directory name.
    pub fn basename(&self) -> Option<&str> {
        self.components.last().map(|s| s.as_str())
    }

    /// Convert the portable `Path` into a platform `PathBuf`.
    ///
    /// Components that are `.` or `..` are ignored to produce a clean
    /// `PathBuf` suitable for filesystem operations.
    pub fn append_to(&self, base_dir: &StdPath) -> PathBuf {
        let mut ret = base_dir.to_owned();
        for comp in &self.components {
            ret.push(comp);
        }
        ret
    }

    /// Retrieve the `FileStat` for this portable path.
    ///
    /// This will convert the portable path into a `PathBuf` and check for the
    /// file's existence. If the path exists the file metadata is returned as
    /// a `FileStat`, otherwise an `Error::InvalidPath` is returned.
    #[cfg(not(target_arch = "wasm32"))]
    async fn get_file_stat(&self, base_dir: &StdPath) -> Result<FileStat, Error> {
        let path: PathBuf = base_dir.into();
        self.append_to(&path);
        if path.exists() {
            Ok(FileStat::from_path(path.as_path()).await?)
        } else {
            Err(Error::InvalidPath {
                what: format!("path doesn't exists:{}", path.display()),
            })
        }
    }

    /// Looks up the metadata for the current portable path.
    ///
    /// # Returns
    /// * `Result<Lookup, Error>` - The lookup result containing the path and
    ///   its metadata, or an error message.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn lookup(&self, base_dir: &StdPath) -> Result<FileInfo, Error> {
        Ok(FileInfo {
            path: self.clone(),
            stats: self.get_file_stat(base_dir).await?,
        })
    }

    /// Returns the parent path of the current `PortablePath`, or `None` if
    /// there is no parent.
    pub fn parent(&self) -> Option<Path> {
        if self.components.is_empty() {
            None
        } else {
            let mut parent_components = self.components.clone();
            parent_components.pop();
            Some(Path {
                components: parent_components,
            })
        }
    }

    /// Appends a new component to the end of the portable path.
    ///
    /// # Arguments
    ///
    /// * `component` - The path component to add.
    pub fn push(&mut self, component: &str) {
        self.components.push(component.to_owned());
    }

    /// Join two PortablePaths together into a new PortablePath.
    ///
    /// # Arguments
    /// * `other` - The other PortablePath to join with.
    ///
    /// # Returns
    /// * `PortablePath` - A new PortablePath representing the joined path.
    pub fn join(&self, other: &Path) -> Path {
        let mut ret = self.clone();
        for comp in &other.components {
            ret.push(comp);
        }
        ret
    }

    /// Verifies if the file exists
    pub fn is_valid(&self, base_dir: &StdPath) -> bool {
        self.append_to(base_dir).exists()
    }
}

impl<T> TryFrom<&[T]> for Path
where
    T: AsRef<str>,
{
    type Error = Error;

    /// Attempt to build a `Path` from a slice of components.
    ///
    /// Each component is validated to not contain directory separators and to
    /// not equal `.` or `..`. Returns `Error::InvalidArgument` on invalid
    /// components.
    fn try_from(components: &[T]) -> std::result::Result<Self, Self::Error> {
        let mut c = Vec::new();
        for comp in components {
            let s = comp.as_ref();
            if s.contains('/') || s.contains('\\') {
                return Err(Error::InvalidArgument(format!(
                    "Invalid path component: {s}"
                )));
            }
            if s == "." || s == ".." || s.is_empty() {
                return Err(Error::InvalidArgument(format!(
                    "Invalid path component: {s}"
                )));
            }
            c.push(s.to_string());
        }
        Ok(Path { components: c })
    }
}

impl TryFrom<&PathBuf> for Path {
    type Error = Error;

    /// Convert a `PathBuf` into the portable `Path` representation.
    ///
    /// This will reject paths that are just `.` or `..` and will strip root
    /// components. Non-UTF8 components will be skipped.
    fn try_from(path: &PathBuf) -> Result<Self, Self::Error> {
        Self::try_from(path.as_path())
    }
}

impl TryFrom<&StdPath> for Path {
    type Error = Error;

    /// Convert a `PathBuf` into the portable `Path` representation.
    ///
    /// This will reject paths that are just `.` or `..` and will strip root
    /// components. Non-UTF8 components will be skipped.
    fn try_from(path: &StdPath) -> Result<Self, Self::Error> {
        let str = path.to_string_lossy();
        if str == "." || str == ".." {
            return Err(Error::InvalidArgument(
                "Path cannot contain '.' or '..' components".to_string(),
            ));
        }
        let components = path
            .components()
            .filter_map(|comp| {
                let s = comp.as_os_str().to_str()?;
                if s == std::path::Component::RootDir.as_os_str().to_str().unwrap() {
                    None
                } else {
                    Some(s.to_string())
                }
            })
            .collect();
        Ok(Path { components })
    }
}

#[cfg(test)]
mod tests {
    use crate::Path;

    #[test]
    fn root_path_display() {
        assert_eq!(Path::try_from(["a"].as_slice()).unwrap().to_string(), "a");
    }

    #[test]
    fn empty_path_display() {
        assert_eq!(Path::empty().to_string(), "");
    }

    #[test]
    fn two_components_path_display() {
        assert_eq!(
            Path::try_from(["a", "b"].as_slice()).unwrap().to_string(),
            "a/b"
        );
    }

    #[test]
    fn three_components_path_display() {
        assert_eq!(
            Path::try_from(["a", "b", "c"].as_slice())
                .unwrap()
                .to_string(),
            "a/b/c"
        );
    }
}
