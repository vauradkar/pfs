#[cfg(feature = "poem")]
use poem_openapi::Object;
#[cfg(feature = "json_schema")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::FileInfo;
use crate::Path;

/// A list of files and directories contained in `base_dir`
#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct RecursiveDirList {
    /// Path where the directory should be synced
    pub base_dir: Path,
    /// List of file info representing in the `base_dir` directory tree
    pub deltas: Vec<FileInfo>,
}
