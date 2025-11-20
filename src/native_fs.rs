use std::collections::HashMap;
use std::path::Path as StdPath;
use std::path::PathBuf;
use std::sync::Arc;

use async_recursion::async_recursion;
use futures_lite::StreamExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use crate::Error;
use crate::FileInfo;
use crate::FileStat;
use crate::Path;
use crate::filter::FilterLevel;
use crate::portable_fs::FsLayer;
use crate::portable_fs::lookup_or_load;

pub(crate) struct DirWalker {
    strip_prefix: PathBuf,
    layer: Arc<FsLayer>,
    chunk_size: usize,
    max_depth: Option<usize>,
    tx: Sender<Vec<FileInfo>>,
    lookup: HashMap<PathBuf, FileStat>,
}

impl DirWalker {
    pub fn create<P: AsRef<StdPath>>(
        strip_prefix: P,
        layer: Arc<FsLayer>,
        chunk_size: usize,
        max_depth: Option<usize>,
        tx: Sender<Vec<FileInfo>>,
        lookup: HashMap<PathBuf, FileStat>,
    ) -> Self {
        Self {
            strip_prefix: strip_prefix.as_ref().to_path_buf(),
            layer,
            chunk_size,
            max_depth,
            tx,
            lookup,
        }
    }

    pub async fn walk_dir<P: AsRef<StdPath>>(
        full_path: P,
        strip_prefix: P,
        layer: Arc<FsLayer>,
        chunk_size: usize,
        max_depth: Option<usize>,
    ) -> Result<Vec<FileInfo>, Error> {
        let full_path = full_path.as_ref().to_path_buf();
        let strip_prefix = strip_prefix.as_ref().to_path_buf();
        let (tx, mut rx) = mpsc::channel(100);
        let x = tokio::spawn(async move {
            let dir_walker = DirWalker::create(
                strip_prefix,
                layer,
                chunk_size,
                max_depth,
                tx,
                HashMap::new(),
            );
            dir_walker.walk_dir_stream(&full_path).await
        });
        let mut items = Vec::new();
        while let Some(mut item) = rx.recv().await {
            items.append(&mut item);
        }
        x.await.map_err(|e| Error::Read {
            what: "failed to join walk_dir thread".to_owned(),
            how: e.to_string(),
        })??;
        Ok(items)
    }

    async fn write_chunks(&self, chunks: &mut Vec<FileInfo>) -> Result<(), Error> {
        self.tx
            .send(std::mem::take(chunks))
            .await
            .map_err(|e| Error::Sync {
                what: "failed to tx".to_owned(),
                how: e.to_string(),
            })?;
        if chunks.capacity() < self.chunk_size {
            chunks.reserve(self.chunk_size - chunks.capacity());
        }
        Ok(())
    }

    async fn push_and_send(&self, chunks: &mut Vec<FileInfo>, item: FileInfo) -> Result<(), Error> {
        chunks.push(item);
        if chunks.len() == self.chunk_size {
            self.write_chunks(chunks).await?;
        }
        Ok(())
    }

    /// Walk a directory tree up to a specified depth
    pub async fn walk_dir_stream<P: AsRef<StdPath>>(&self, full_path: &P) -> Result<(), Error> {
        let mut chunks = Vec::with_capacity(self.chunk_size);
        self.walk_recursive(full_path.as_ref(), 0, &mut chunks)
            .await?;
        Ok(())
    }

    #[async_recursion]
    async fn walk_recursive(
        &self,
        dir_path: &StdPath,
        current_depth: usize,
        chunks: &mut Vec<FileInfo>,
    ) -> Result<(), Error> {
        // Stop if we've reached max depth
        if current_depth > *self.max_depth.as_ref().unwrap_or(&usize::MAX) {
            return Ok(());
        }

        // Read directory entries
        let mut entries = async_fs::read_dir(&dir_path)
            .await
            .map_err(|e| Error::Read {
                what: dir_path.to_string_lossy().to_string(),
                how: e.to_string(),
            })?;

        // Process each entry
        while let Some(entry) = entries.next().await {
            let entry = entry.map_err(|e| Error::Read {
                what: "walkdir".into(),
                how: e.to_string(),
            })?;
            let entry_path = entry.path();

            let relative_path = entry_path
                .strip_prefix(&self.strip_prefix)
                .map_err(|e| Error::Read {
                    what: "strip_prefix".into(),
                    how: e.to_string(),
                })?
                .to_owned();
            let portable_path = Path::try_from(&relative_path)?;
            let stats = lookup_or_load(self.layer.clone(), &entry_path, &portable_path).await?;
            let is_dir = stats.is_directory;
            let filter_level = self
                .layer
                .filter_set
                .read()
                .unwrap()
                .matches(&relative_path, is_dir)
                .unwrap();
            if filter_level == FilterLevel::Deny {
                continue;
            } else if filter_level == FilterLevel::Allow {
                let skip_push = self
                    .lookup
                    .get(&relative_path)
                    .map(|s| s == &stats)
                    .unwrap_or(false);
                if !skip_push {
                    self.push_and_send(
                        chunks,
                        FileInfo {
                            path: portable_path,
                            stats,
                        },
                    )
                    .await?;
                }
            }

            if !is_dir {
                continue;
            }

            // Recursively walk subdirectories
            self.walk_recursive(&entry_path, current_depth + 1, chunks)
                .await?;
        }

        if !chunks.is_empty() {
            self.write_chunks(chunks).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::num::NonZero;

    use super::*;
    use crate::TestRoot;
    use crate::cache::NullCache;
    use crate::filter::FilterSet;
    async fn setup_test(fset: FilterSet) -> (TestRoot, Vec<FileInfo>) {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let full_path = root.root.path();
        let strip_prefix = full_path;
        let layer = Arc::new(FsLayer::new(
            Box::new(NullCache::new(NonZero::new(100).unwrap())),
            fset,
        ));

        let flist = DirWalker::walk_dir(full_path, strip_prefix, layer, 2, None)
            .await
            .unwrap();
        (root, flist)
    }

    #[tokio::test]
    async fn test_selective_allow() {
        let mut fset = FilterSet::new();
        fset.allow_path("dir1");
        let (_root, flist) = setup_test(fset).await;

        for info in flist {
            assert!(
                info.path.to_string().starts_with("dir1"),
                "for {}",
                info.path
            );
        }
    }

    fn check_expected(found: &[FileInfo], expected: &[&str]) {
        let mut expected: HashSet<String> = expected.iter().map(|s| s.to_string()).collect();

        for info in found {
            assert!(expected.remove(&info.path.to_string()), "for {}", info.path);
        }
        assert!(expected.is_empty(), "{:?}", expected);
    }

    #[tokio::test]
    async fn test_selective_deny() {
        let mut fset = FilterSet::new();
        fset.allow_path("dir1");
        fset.allow_extension("txt");
        let (_root, flist) = setup_test(fset).await;

        for info in flist {
            assert!(
                info.path.to_string().starts_with("dir1"),
                "for {}",
                info.path
            );
        }
    }

    #[tokio::test]
    async fn test_allow_ext() {
        let mut fset = FilterSet::new();
        fset.allow_extension("txt");
        fset.allow_extension("rs");
        let (_root, flist) = setup_test(fset).await;

        let expected = [
            "file1.txt",
            "file2.txt",
            "dir1/file3.txt",
            "dir1/dir2/file4.txt",
            "dir3/file6.txt",
            "dir1/file8.rs",
        ];
        check_expected(&flist, &expected);
    }

    #[tokio::test]
    async fn test_selective_deny_with_ext() {
        let mut fset = FilterSet::new();
        fset.allow_extension("txt");
        fset.deny_path("dir3");
        let (_root, flist) = setup_test(fset).await;

        let expected = [
            "file1.txt",
            "file2.txt",
            "dir1/file3.txt",
            "dir1/dir2/file4.txt",
        ];
        check_expected(&flist, &expected);
    }

    #[tokio::test]
    async fn test_allow_and_deny() {
        let mut fset = FilterSet::new();
        fset.allow_path("dir1");
        fset.deny_path("dir1/dir2");

        let (_root, flist) = setup_test(fset).await;

        let expected = ["dir1/file3.txt", "dir1", "dir1/file7.md", "dir1/file8.rs"];
        check_expected(&flist, &expected);
    }

    #[tokio::test]
    async fn test_allow_denied() {
        let mut fset = FilterSet::new();
        fset.allow_path("dir1/dir2");
        fset.deny_path("dir1");

        let (_root, flist) = setup_test(fset).await;

        let expected = [];
        check_expected(&flist, &expected);
    }
}
