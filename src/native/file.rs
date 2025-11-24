use std::path::Path as StdPath;

use async_fs::DirEntry;

use crate::FileStat;
use crate::errors::Error;
use crate::hash::Sha256Builder;
use crate::hash::Sha256String;

impl FileStat {
    /// Creates a `FileStat` from a directory entry, including digest for files.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn from_dir_entry(entry: &DirEntry) -> Result<Self, Error> {
        let path = entry.path();
        Self::from_path(&path).await
    }

    /// Creates a `FileStat` from a directory entry, including digest for files.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn from_path<P: AsRef<StdPath>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref();
        let metadata = tokio::fs::metadata(&path).await.map_err(|e| Error::Read {
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
}
