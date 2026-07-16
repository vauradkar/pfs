use std::path::Path as StdPath;

use async_fs::DirEntry;

use crate::FileStat;
use crate::errors::Error;
use crate::hash::Sha256Builder;
use crate::hash::Sha256String;

impl FileStat {
    /// Creates a `FileStat` from a directory entry, including digest for
    /// files when `with_sha` is `true`.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn from_dir_entry(entry: &DirEntry, with_sha: bool) -> Result<Self, Error> {
        let path = entry.path();
        Self::from_path(&path, with_sha).await
    }

    /// Creates a `FileStat` from a path, including digest for files when
    /// `with_sha` is `true`. Directories never have a digest.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn from_path<P: AsRef<StdPath>>(path: P, with_sha: bool) -> Result<Self, Error> {
        let path = path.as_ref();
        let metadata = tokio::fs::metadata(&path).await.map_err(|e| Error::Read {
            what: "metadata".into(),
            how: e.to_string(),
        })?;
        if metadata.is_dir() || !with_sha {
            Ok(FileStat::from_metadata(&metadata, None))
        } else {
            let sha256 = path.sha256_build().await?.sha256_string().await?;
            Ok(FileStat::from_metadata(&metadata, Some(sha256)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir::TempDir;

    #[tokio::test]
    async fn from_path_computes_sha256_when_with_sha() {
        let temp_dir = TempDir::new("pfs-file-stat").unwrap();
        let file_path = temp_dir.path().join("sample.bin");
        std::fs::write(&file_path, b"hello world").unwrap();

        let stat = FileStat::from_path(&file_path, true).await.unwrap();
        let expected = file_path
            .as_path()
            .sha256_build()
            .await
            .unwrap()
            .sha256_string()
            .await
            .unwrap();

        assert_eq!(stat.sha256.as_deref(), Some(expected.as_str()));
    }

    #[tokio::test]
    async fn from_path_skips_sha256_when_not_with_sha() {
        let temp_dir = TempDir::new("pfs-file-stat").unwrap();
        let file_path = temp_dir.path().join("sample.bin");
        std::fs::write(&file_path, b"hello world").unwrap();

        let stat = FileStat::from_path(&file_path, false).await.unwrap();

        assert_eq!(stat.sha256, None);
    }

    #[tokio::test]
    async fn from_path_never_computes_sha256_for_directories() {
        let temp_dir = TempDir::new("pfs-file-stat").unwrap();

        let stat = FileStat::from_path(temp_dir.path(), true).await.unwrap();

        assert!(stat.is_directory);
        assert_eq!(stat.sha256, None);
    }
}
