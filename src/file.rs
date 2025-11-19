use std::fs::Metadata;
use std::path::Path as StdPath;
use std::time::SystemTime;

use async_fs::DirEntry;
#[cfg(feature = "poem")]
use poem_openapi::Object;
#[cfg(feature = "json_schema")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use tokio::fs;

use crate::errors::Error;
use crate::hash::Sha256Builder;
use crate::hash::Sha256String;
use crate::path::Path;
use crate::utils::format_system_time;

/// Represents the metadata of a file or directory, including its path, size,
/// modification time, and type.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct FileStat {
    /// The size of the file in bytes. For directories, this may be zero or
    /// implementation-defined.
    pub size: u64,
    /// The last modification time of the file or directory in RFC 3339 - Z
    /// format. For example "2018-01-26T18:30:09.453Z"
    pub mtime: String,
    /// Whether this entry is a directory.
    pub is_directory: bool,
    /// Optional digest of the file contents.
    /// This allows us faster directory browsing.
    pub sha256: Option<String>,
}

impl FileStat {
    /// Creates a `FileStat` from a directory entry, including digest for files.
    pub async fn from_dir_entry(entry: &DirEntry) -> Result<Self, Error> {
        let path = entry.path();
        Self::from_path(&path).await
    }

    /// Creates a `FileStat` from a directory entry, including digest for files.
    pub async fn from_path<P: AsRef<StdPath>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref();
        let metadata = fs::metadata(&path).await.map_err(|e| Error::Read {
            what: "metadata".into(),
            how: e.to_string(),
        })?;
        if metadata.is_dir() {
            Ok(FileStat::from_metadata(&metadata, Some("".to_string())))
        } else {
            let sha256 = path.sha256_build().await?.sha256_string().await?;
            Ok(FileStat::from_metadata(&metadata, Some(sha256)))
        }
    }

    /// Create a `FileStat` from a `Metadata` value and an optional sha256.
    ///
    /// This helper extracts the file size, modification time and directory
    /// flag from the provided metadata and formats the modification time
    /// using `format_system_time`. The optional `sha256` can be set to `None`
    /// for directories or omitted values.
    pub fn from_metadata(metadata: &Metadata, sha256: Option<String>) -> Self {
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        FileStat {
            size: metadata.len(),
            mtime: format_system_time(modified),
            is_directory: metadata.is_dir(),
            sha256,
        }
    }
}

/// Represents the contents of a directory, including the current path and its
/// items.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct FileInfo {
    /// The full path of the file.
    pub path: Path,
    /// Metadata if the file exists.
    pub stats: FileStat,
}

/// Akin to inode, represents the a file or directory, including its path, size,
/// modification time, type and contents.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct FileNode {
    /// The metadata of the file or directory.
    pub stats: FileStat,
    /// The contents of the file. Empty for directories.
    pub contents: Vec<u8>,
}

impl FileNode {
    /// Creates a new `FileNode` instance with the specified metadata and
    /// contents.
    pub fn new(stats: FileStat, contents: Vec<u8>) -> Self {
        Self { stats, contents }
    }
}

impl From<(FileStat, Vec<u8>)> for FileNode {
    fn from(tuple: (FileStat, Vec<u8>)) -> Self {
        Self::new(tuple.0, tuple.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path() {
        let path = Path::try_from(["dir1", "dir2", "file.txt"].as_slice()).unwrap();
        let s = "{\"components\":[\"dir1\",\"dir2\",\"file.txt\"]}";
        assert_eq!(s, &serde_json::to_string(&path).unwrap());
        assert_eq!(path, serde_json::from_str(s).unwrap());
    }
}
