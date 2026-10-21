use std::fmt::Display;
use std::path::Component;
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

/// Checks that `component` is a single, plain path component.
///
/// A `Path` is always joined onto a base directory, so every component must
/// stay inside it. Empty components, `.`, `..`, components containing a
/// separator (`/` or `\`) and anything the platform does not treat as a single
/// normal component (for example an absolute path or a Windows drive prefix
/// such as `C:`) are rejected.
pub(crate) fn validate_component(component: &str) -> Result<(), Error> {
    let mut parts = StdPath::new(component).components();
    let is_plain = !component.contains(['/', '\\'])
        && matches!(parts.next(), Some(Component::Normal(c)) if c == component)
        && parts.next().is_none();
    if is_plain {
        Ok(())
    } else {
        Err(Error::InvalidArgument(format!(
            "Invalid path component: {component:?}"
        )))
    }
}

/// A custom deserializer function for a Vec<String> that rejects components
/// which could escape the base directory. See [`validate_component`].
fn deserialize_components<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let components = Vec::<String>::deserialize(deserializer)?;
    for component in &components {
        validate_component(component).map_err(de::Error::custom)?;
    }
    Ok(components)
}

/// poem-openapi validator applying [`validate_component`] to each component.
///
/// poem-openapi parses request bodies with its own `ParseFromJSON`
/// implementation rather than serde, so `deserialize_components` alone does
/// not protect API endpoints.
#[cfg(feature = "poem")]
struct ComponentValidator;

#[cfg(feature = "poem")]
impl Display for ComponentValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid path component")
    }
}

#[cfg(feature = "poem")]
impl poem_openapi::validation::Validator<String> for ComponentValidator {
    fn check(&self, value: &String) -> bool {
        validate_component(value).is_ok()
    }
}

/// Represents a filesystem path as a vector of its portable components.
/// `Path` in itself is useless. It is a base/root path to be useful.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct Path {
    /// The components of the portable path as a vector of strings.
    #[serde(deserialize_with = "deserialize_components")]
    #[cfg_attr(feature = "poem", oai(validator(custom = "ComponentValidator")))]
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
        if !self.components.is_empty() {
            let reserve = self.components.iter().map(|comp| comp.len() + 1).sum();
            ret.reserve(reserve);
        }
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
            Ok(FileStat::from_path(path.as_path(), true).await?)
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
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidArgument` if `component` is not a single plain
    /// path component. See [`validate_component`].
    pub fn push(&mut self, component: &str) -> Result<(), Error> {
        validate_component(component)?;
        self.components.push(component.to_owned());
        Ok(())
    }

    /// Join two PortablePaths together into a new PortablePath.
    ///
    /// # Arguments
    /// * `other` - The other PortablePath to join with.
    ///
    /// # Returns
    /// * `PortablePath` - A new PortablePath representing the joined path.
    pub fn join(&self, other: &Path) -> Path {
        // Both paths only hold validated components.
        let mut ret = self.clone();
        ret.components.extend(other.components.iter().cloned());
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
    /// Each component is validated with [`validate_component`]. Returns
    /// `Error::InvalidArgument` on invalid components.
    fn try_from(components: &[T]) -> std::result::Result<Self, Self::Error> {
        let mut c = Vec::new();
        for comp in components {
            let s = comp.as_ref();
            validate_component(s)?;
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
    /// This will reject paths containing `..` or a Windows prefix (such as
    /// `C:`), and paths that are just `.`. Root and `.` components are
    /// stripped. Non-UTF8 components will be skipped.
    fn try_from(path: &StdPath) -> Result<Self, Self::Error> {
        if path.as_os_str() == "." {
            return Err(Error::InvalidArgument(
                "Path cannot contain '.' or '..' components".to_string(),
            ));
        }
        let mut components = Vec::new();
        for comp in path.components() {
            match comp {
                Component::Normal(s) => {
                    if let Some(s) = s.to_str() {
                        components.push(s.to_string());
                    }
                }
                Component::RootDir | Component::CurDir => {}
                Component::ParentDir | Component::Prefix(_) => {
                    return Err(Error::InvalidArgument(format!(
                        "Path cannot contain '..' or prefix components: {}",
                        path.display()
                    )));
                }
            }
        }

        Ok(Path { components })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path as StdPath;

    use super::validate_component;
    use crate::Path;

    /// Components that would escape, or replace, the base directory when the
    /// path is joined onto it.
    const ESCAPING_COMPONENTS: &[&str] = &[
        "",
        ".",
        "..",
        "/",
        "/etc",
        "/etc/passwd",
        "a/b",
        "a/../..",
        "../secret",
        "a\\b",
        "..\\secret",
        "\\\\server\\share",
        #[cfg(windows)]
        "C:",
        #[cfg(windows)]
        "C:\\Windows",
    ];

    const VALID_COMPONENTS: &[&str] = &[
        "a",
        "file.txt",
        ".hidden",
        "..a",
        "a..",
        "...",
        "with space",
        "ünïcödé",
        #[cfg(not(windows))]
        "a:b",
    ];

    fn from_json(components: &[&str]) -> Result<Path, serde_json::Error> {
        serde_json::from_value(serde_json::json!({ "components": components }))
    }

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

    #[test]
    fn validate_component_rejects_escaping_components() {
        for c in ESCAPING_COMPONENTS {
            assert!(validate_component(c).is_err(), "accepted {c:?}");
        }
    }

    #[test]
    fn validate_component_accepts_plain_components() {
        for c in VALID_COMPONENTS {
            assert!(validate_component(c).is_ok(), "rejected {c:?}");
        }
    }

    #[test]
    fn try_from_slice_rejects_escaping_components() {
        for c in ESCAPING_COMPONENTS {
            assert!(Path::try_from(["a", c].as_slice()).is_err(), "accepted {c:?}");
        }
    }

    #[test]
    fn deserialize_rejects_escaping_components() {
        for c in ESCAPING_COMPONENTS {
            assert!(from_json(&["a", c]).is_err(), "accepted {c:?}");
        }
    }

    #[test]
    fn deserialize_accepts_plain_components() {
        let path = from_json(VALID_COMPONENTS).unwrap();
        assert_eq!(path, Path::try_from(VALID_COMPONENTS).unwrap());
        assert_eq!(from_json(&[]).unwrap(), Path::empty());
    }

    #[cfg(feature = "poem")]
    #[test]
    fn poem_parse_rejects_escaping_components() {
        use poem_openapi::types::ParseFromJSON;

        for c in ESCAPING_COMPONENTS {
            let value = serde_json::json!({ "components": ["a", c] });
            assert!(Path::parse_from_json(Some(value)).is_err(), "accepted {c:?}");
        }
    }

    #[cfg(feature = "poem")]
    #[test]
    fn poem_parse_accepts_plain_components() {
        use poem_openapi::types::ParseFromJSON;

        let value = serde_json::json!({ "components": VALID_COMPONENTS });
        assert_eq!(
            Path::parse_from_json(Some(value)).unwrap(),
            Path::try_from(VALID_COMPONENTS).unwrap()
        );
    }

    #[test]
    fn push_rejects_escaping_components() {
        let original = Path::try_from(["a"].as_slice()).unwrap();
        for c in ESCAPING_COMPONENTS {
            let mut path = original.clone();
            assert!(path.push(c).is_err(), "accepted {c:?}");
            assert_eq!(path, original);
        }
    }

    #[test]
    fn push_appends_plain_component() {
        let mut path = Path::try_from(["a"].as_slice()).unwrap();
        path.push("b.txt").unwrap();
        assert_eq!(path, Path::try_from(["a", "b.txt"].as_slice()).unwrap());
    }

    #[test]
    fn try_from_std_path_rejects_parent_dir() {
        for p in ["..", "../a", "a/..", "a/../../b", "/a/../b"] {
            assert!(Path::try_from(StdPath::new(p)).is_err(), "accepted {p:?}");
        }
    }

    #[test]
    fn try_from_std_path_strips_root_and_cur_dir() {
        let expected = Path::try_from(["a", "b"].as_slice()).unwrap();
        for p in ["a/b", "/a/b", "./a/b", "a/./b"] {
            assert_eq!(Path::try_from(StdPath::new(p)).unwrap(), expected, "{p:?}");
        }
        assert!(Path::try_from(StdPath::new(".")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn try_from_std_path_rejects_prefix() {
        assert!(Path::try_from(StdPath::new("C:\\a")).is_err());
    }

    #[test]
    fn append_to_stays_inside_base() {
        let base = StdPath::new("/base");
        for c in VALID_COMPONENTS {
            let path = from_json(&["dir", c]).unwrap();
            let full = path.append_to(base);
            assert!(full.starts_with(base), "{c:?} -> {}", full.display());
            assert_eq!(full.strip_prefix(base).unwrap().components().count(), 2);
        }
    }
}
