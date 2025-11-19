use std::fs::DirEntry;

#[cfg(feature = "poem")]
use poem_openapi::Object;
#[cfg(feature = "json_schema")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::Error;
use crate::FileInfo;
use crate::FileStat;
use crate::Path;

/// Represents a file or directory entry, including its name and associated
/// metadata.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct DirectoryEntry {
    /// Name of the file or directory.
    pub name: String,
    /// Metadata of the file or directory.
    pub stats: FileStat,
}

impl TryFrom<&FileInfo> for DirectoryEntry {
    type Error = Error;
    fn try_from(item: &FileInfo) -> Result<Self, crate::Error> {
        let name = item
            .path
            .basename()
            .ok_or(Error::InvalidPath {
                what: item.path.to_string(),
            })?
            .to_string();
        let stats = item.stats.clone();
        Ok(DirectoryEntry { name, stats })
    }
}

impl TryFrom<&DirEntry> for DirectoryEntry {
    type Error = Error;
    fn try_from(entry: &DirEntry) -> Result<Self, crate::Error> {
        let metadata = std::fs::metadata(entry.path()).map_err(|e| Error::Read {
            what: "metadata".into(),
            how: e.to_string(),
        })?;
        Ok(Self {
            name: entry.file_name().into_string().unwrap(),
            stats: FileStat::from_metadata(&metadata, None),
        })
    }
}
/// Represents the contents of a directory, including the current path and its
/// items.
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct Directory {
    /// The current directory path.
    pub current_path: Path,
    /// The list of files and directories in the current path.
    pub items: Vec<DirectoryEntry>,
}
