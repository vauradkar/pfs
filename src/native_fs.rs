use std::collections::HashMap;
use std::path::Path as StdPath;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use async_recursion::async_recursion;
use futures_lite::StreamExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use crate::Error;
use crate::FileInfo;
use crate::FileStat;
use crate::Path;
use crate::cache::Cache;
use crate::portable_fs::lookup_or_load;

pub(crate) struct DirWalker {
    strip_prefix: PathBuf,
    cache: Arc<Mutex<Box<dyn Cache>>>,
    chunk_size: usize,
    max_depth: Option<usize>,
    tx: Sender<Vec<FileInfo>>,
    lookup: HashMap<PathBuf, FileStat>,
}

impl DirWalker {
    pub fn create<P: AsRef<StdPath>>(
        strip_prefix: P,
        cache: Arc<Mutex<Box<dyn Cache>>>,
        chunk_size: usize,
        max_depth: Option<usize>,
        tx: Sender<Vec<FileInfo>>,
        lookup: HashMap<PathBuf, FileStat>,
    ) -> Self {
        Self {
            strip_prefix: strip_prefix.as_ref().to_path_buf(),
            cache,
            chunk_size,
            max_depth,
            tx,
            lookup,
        }
    }

    pub async fn walk_dir<P: AsRef<StdPath>>(
        full_path: P,
        strip_prefix: P,
        cache: Arc<Mutex<Box<dyn Cache>>>,
        chunk_size: usize,
        max_depth: Option<usize>,
    ) -> Result<Vec<FileInfo>, Error> {
        let full_path = full_path.as_ref().to_path_buf();
        let strip_prefix = strip_prefix.as_ref().to_path_buf();
        let (tx, mut rx) = mpsc::channel(100);
        let x = tokio::spawn(async move {
            let dir_walker = DirWalker::create(
                strip_prefix,
                cache,
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
            let stats = lookup_or_load(self.cache.clone(), &entry_path, &portable_path).await?;
            let is_dir = stats.is_directory;
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
