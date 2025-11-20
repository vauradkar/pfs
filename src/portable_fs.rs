use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path as StdPath;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::RwLock;

use log::debug;
use log::error;
use tokio::sync::mpsc::Sender;

use crate::Directory;
use crate::DirectoryEntry;
use crate::Error;
use crate::FileInfo;
use crate::FileStat;
use crate::Path;
use crate::RecursiveDirList;
use crate::cache::Cache;
use crate::cache::FsCache;
use crate::cache::NullCache;
use crate::filter::FilterSet;
use crate::native_fs::DirWalker;
use crate::utils::parse_system_time;

pub(crate) async fn lookup_or_load(
    layer: Arc<FsLayer>,
    path: &StdPath,
    portable_path: &Path,
) -> Result<FileStat, Error> {
    if let Some(stats) = layer.cache.lock().unwrap().get(portable_path) {
        Ok(stats.clone())
    } else {
        let stats = FileStat::from_path(path).await?;
        layer
            .cache
            .lock()
            .unwrap()
            .put(portable_path.clone(), stats.clone());
        Ok(stats)
    }
}

/// Caching and filtering layers that sit above and below the `PortableFs`
#[derive(Clone)]
pub(crate) struct FsLayer {
    cache: Arc<Mutex<Box<dyn Cache>>>,
    pub filter_set: Arc<RwLock<FilterSet>>,
}

impl FsLayer {
    /// Creates a new FsLayer from given `cache` and `filter_set`
    pub fn new(cache: Box<dyn Cache>, filter_set: FilterSet) -> Self {
        Self {
            cache: Arc::new(Mutex::new(cache)),
            filter_set: Arc::new(RwLock::new(filter_set)),
        }
    }
}

/// Represents a filesystem rooted at a relative base directory.
#[derive(Clone)]
pub struct PortableFs {
    // The relative path from the base directory.
    base_dir: PathBuf,
    layer: Arc<FsLayer>,
}

impl PortableFs {
    fn with(base_dir: PathBuf, cache: Box<dyn Cache>) -> Self {
        PortableFs {
            base_dir,
            layer: Arc::new(FsLayer::new(cache, FilterSet::new())),
        }
    }

    /// creates portable fs with cache
    pub fn with_cache(base_dir: PathBuf) -> Self {
        Self::with(
            base_dir,
            Box::new(FsCache::new(NonZeroUsize::new(1000).unwrap())),
        )
    }

    /// creates portable fs with out cache
    pub fn without_cache(base_dir: PathBuf) -> Self {
        Self::with(
            base_dir,
            Box::new(NullCache::new(NonZeroUsize::new(1000).unwrap())),
        )
    }

    /// Converts a relative Path to an absolute PathBuf based on the
    /// base_dir.
    ///
    /// # Arguments
    /// * `relative` - The relative Path to convert.
    ///
    /// # Returns
    /// * `PathBuf` - The absolute path corresponding to the relative path.
    pub fn as_abs_path(&self, relative: &Path) -> PathBuf {
        relative.append_to(&self.base_dir)
    }

    /// Converts a relative Path to a PathBuf relative to the root (empty base).
    ///
    /// # Arguments
    /// * `relative` - The relative Path to convert.
    ///
    /// # Returns
    /// * `PathBuf` - The path corresponding to the relative path from the root.
    pub fn as_relative_path(&self, relative: &Path) -> PathBuf {
        relative.append_to(StdPath::new(""))
    }

    /// Read the contents of the given directory path and returns its
    /// entries.
    ///
    /// # Arguments
    /// * `path` - The path to the directory to browse.
    ///
    /// # Returns
    /// * `Result<Directory, Error>` - The directory entries or an error
    ///   message.
    pub async fn read_dir(&self, path: &Path) -> Result<Directory, Error> {
        let full_path = self.as_abs_path(path);
        let mut items = Vec::new();
        for item in DirWalker::walk_dir(
            full_path,
            self.base_dir.clone(),
            self.layer.clone(),
            20,
            Some(0),
        )
        .await?
        {
            items.push(DirectoryEntry::try_from(&item)?);
        }

        // Sort: directories first, then files, both alphabetically
        items.sort_by(|a, b| match (a.stats.is_directory, b.stats.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        Ok(Directory {
            current_path: path.clone(),
            items,
        })
    }

    /// Recursively walks directory `path` and returns files and their metadata
    /// under the directory tree.
    ///
    /// # Arguments
    /// * `path` - The path to the directory to browse.
    ///
    /// # Returns
    /// * `Result<Vec<FileInfo>, Error>` - The directory entries or an error
    ///   message.
    pub async fn read_dir_recurse(&self, path: &Path) -> Result<Vec<FileInfo>, Error> {
        DirWalker::walk_dir(
            self.as_abs_path(path),
            self.base_dir.clone(),
            self.layer.clone(),
            20,
            None,
        )
        .await
    }

    /// Exchanges file deltas by sending FileInfo objects for the given
    /// destination path over the provided channel.
    ///
    /// # Arguments
    /// * `tx` - The channel sender to transmit FileInfo objects.
    /// * `delta` - The DeltaRequest containing the destination path to recurse.
    /// * `chunk_size` - max size of a chunk before it is sent across the
    ///   channel
    pub async fn exchange_deltas(
        &self,
        tx: Sender<Vec<FileInfo>>,
        delta: RecursiveDirList,
        chunk_size: usize,
    ) {
        let full_path = self.as_abs_path(&delta.base_dir);
        let strip_prefix = if let Some(parent) = delta.base_dir.parent() {
            self.as_abs_path(&parent)
        } else {
            full_path.clone()
        };
        let mut lookup: HashMap<PathBuf, FileStat> = HashMap::new();
        for item in delta.deltas {
            let stats = item.stats;
            lookup.insert(self.as_relative_path(&item.path), stats);
        }
        debug!(
            "exchange_deltas base_dir: {} full_path:{} dest:{}",
            self.base_dir.display(),
            full_path.display(),
            delta.base_dir
        );
        let dir_walker = DirWalker::create(
            strip_prefix,
            self.layer.clone(),
            chunk_size,
            None,
            tx,
            lookup,
        );
        if let Err(e) = dir_walker.walk_dir_stream(&full_path).await {
            error!("exchange_deltas error: {}", e);
        }
    }

    async fn create_all(&self, path: &Path) -> Result<(), String> {
        let full_path = self.as_abs_path(path);
        tokio::fs::create_dir_all(&full_path).await.map_err(|e| {
            error!("Failed to create directory {} {}", e, full_path.display());
            format!("Failed to create directories: {e}")
        })?;
        Ok(())
    }

    /// Writes data to a file at the specified path, optionally overwriting if
    /// the file exists.
    ///
    /// # Arguments
    /// * `path` - The path to the file to write.
    /// * `data` - The data to write to the file.
    /// * `overwrite` - Whether to overwrite the file if it already exists.
    /// * `stats` - value to update the file stats to
    ///
    /// # Returns
    /// * `Result<(), String>` - Ok if successful, or an error message.
    pub async fn write(
        &self,
        path: &Path,
        data: &[u8],
        overwrite: bool,
        stats: &FileStat,
    ) -> Result<(), Error> {
        let full_path = self.as_abs_path(path);
        if full_path.exists() && !overwrite {
            return Err(Error::FileExists(full_path.to_string_lossy().to_string()));
        }

        if let Some(parent) = path.parent() {
            self.create_all(&parent).await.map_err(|e| Error::Create {
                what: path.parent().unwrap().to_string(),
                how: e,
            })?;
        }
        tokio::fs::write(&full_path, data)
            .await
            .map_err(|e| Error::Write {
                what: full_path.to_str().unwrap().into(),
                how: e.to_string(),
            })?;
        let mtime = parse_system_time(&stats.mtime)?;
        let full_path_clone = full_path.clone();
        // Update mtime of the file if stats provided
        let ret = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
            let file = std::fs::File::options()
                .append(true)
                .open(&full_path_clone)?;
            file.set_modified(mtime)
        })
        .await
        .map_err(|e| Error::Write {
            what: full_path.to_str().unwrap().into(),
            how: e.to_string(),
        })?
        .map_err(|e| Error::Write {
            what: full_path.to_str().unwrap().into(),
            how: e.to_string(),
        });
        if ret.is_ok() {
            self.get_cache().put(path.clone(), stats.clone());
        }
        ret
    }

    /// Deletes the file at the specified path.
    pub async fn delete_file(&self, path: &Path) -> Result<(), Error> {
        let full_path = self.as_abs_path(path);
        if !full_path.exists() {
            return Err(Error::InvalidArgument("File does not exist".to_string()));
        }
        if full_path.is_dir() {
            return Err(Error::InvalidArgument("Path is a directory".to_string()));
        }
        let ret = tokio::fs::remove_file(&full_path)
            .await
            .map_err(|e| Error::Delete {
                what: full_path.to_str().unwrap().into(),
                how: e.to_string(),
            });
        if ret.is_ok() {
            self.get_cache().pop(path);
        }
        ret
    }

    /// Reads the contents of the file at the specified path.
    pub async fn read_file(&self, path: &Path) -> Result<Vec<u8>, Error> {
        let full_path = self.as_abs_path(path);
        if !full_path.exists() {
            return Err(Error::InvalidArgument("File does not exist".to_string()));
        }
        if full_path.is_dir() {
            return Err(Error::InvalidArgument("Path is a directory".to_string()));
        }
        tokio::fs::read(&full_path).await.map_err(|e| Error::Read {
            what: full_path.to_str().unwrap().into(),
            how: e.to_string(),
        })
    }

    fn get_cache(&'_ self) -> MutexGuard<'_, Box<dyn Cache>> {
        self.layer.cache.lock().unwrap()
    }

    /// Add new allow filter.
    /// Deny list overrides allow list
    pub fn allow_path<P: AsRef<StdPath>>(&mut self, path: P) {
        self.layer.filter_set.write().unwrap().allow_path(path);
    }

    /// Add new deby filter.
    /// Deny list overrides allow list
    pub fn deny_path<P: AsRef<StdPath>>(&mut self, path: P) {
        self.layer.filter_set.write().unwrap().deny_path(path);
    }

    /// Add an extension to allowed extention list
    pub fn allow_extension(&mut self, ext: &str) {
        self.layer.filter_set.write().unwrap().allow_extension(ext);
    }

    /// Add filename to allowed filename list
    pub fn allow_filename(&mut self, name: &str) {
        self.layer.filter_set.write().unwrap().allow_filename(name);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::SystemTime;

    use tokio::sync::mpsc;

    use super::*;
    use crate::TestRoot;
    use crate::cache::CacheStats;
    use crate::hash::Sha256Builder;
    use crate::hash::Sha256String;
    use crate::utils::format_system_time;

    fn temp_files(root: &TestRoot) -> Vec<String> {
        root.files
            .keys()
            .map(|e| e.as_os_str().to_string_lossy().to_string())
            .collect()
    }

    #[tokio::test]
    async fn test_recurse_path() {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let fs = PortableFs::with_cache(root.root.path().to_path_buf());

        let r = fs
            .read_dir_recurse(&Path::try_from(&StdPath::new("").to_owned()).unwrap())
            .await
            .unwrap();

        println!("r: {r:#?}");
        root.are_synced(&fs, &r).await.unwrap();
    }

    #[tokio::test]
    async fn test_browse_path() {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let fs = PortableFs::with_cache(root.root.path().to_path_buf());

        // Browse the directory
        let portable_path = Path::try_from(&StdPath::new("").to_owned()).unwrap();
        let directory = fs.read_dir(&portable_path).await.unwrap();

        // Assert the directory contains the file
        let mut entries: HashSet<String> = ["file1.txt", "file2.txt", "dir1", "dir3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(directory.items.len(), 4);
        for entry in &directory.items {
            println!("entry name: {}", entry.name);
            assert!(entries.remove(&entry.name));
        }
        assert!(entries.is_empty());
        root.match_entries(&fs, &directory);
    }

    async fn get_deltas(
        path: &str,
        sync_items: Vec<FileInfo>,
    ) -> (HashSet<String>, Vec<FileInfo>, Vec<String>) {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let fs = PortableFs::with_cache(root.root.path().to_path_buf());
        let (a, b) = get_deltas_with(path, sync_items, &fs).await;
        (a, b, temp_files(&root))
    }

    async fn get_deltas_with(
        path: &str,
        sync_items: Vec<FileInfo>,
        fs: &PortableFs,
    ) -> (HashSet<String>, Vec<FileInfo>) {
        // Set up the channel
        let (tx, mut rx) = mpsc::channel(10);
        let delta = RecursiveDirList {
            base_dir: Path::try_from(&StdPath::new(path).to_owned()).unwrap(),
            deltas: sync_items,
        };

        // Call exchange_deltas
        fs.exchange_deltas(tx, delta, 10).await;

        // Assert the channel received the correct FileInfo
        let mut received_items = Vec::new();
        while let Some(items) = rx.recv().await {
            received_items.extend(items);
        }
        println!("received_items: {:#?}", received_items);
        let mut received_files = HashSet::new();
        received_items.iter().for_each(|i| {
            received_files.insert(i.path.to_string());
        });
        (received_files, received_items)
    }

    #[tokio::test]
    async fn test_exchange_deltas_rootdir() {
        let (expected_files, _, temp_files) = get_deltas("", vec![]).await;
        let mut files_found = 0;
        for file in &temp_files {
            files_found += 1;
            assert!(
                expected_files.contains(file),
                "{:?} Missing file: {} ",
                expected_files,
                file
            );
        }
        assert_eq!(
            files_found,
            expected_files.len(),
            "Expected files: {:?}",
            expected_files
        );
    }

    #[tokio::test]
    async fn test_exchange_deltas_subdir() {
        let (expected_files, _, temp_files) = get_deltas("dir1", vec![]).await;
        let mut files_found = 0;
        for file in &temp_files {
            if file.contains("dir1") && file != "dir1" {
                files_found += 1;
                assert!(
                    expected_files.contains(file),
                    "{:?} Missing file: {} ",
                    expected_files,
                    file
                );
            }
        }
        assert_eq!(
            files_found,
            expected_files.len(),
            "Expected files: {:?}",
            expected_files
        );
    }

    #[tokio::test]
    async fn test_exchange_deltas_sends_empty() {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let fs = PortableFs::with_cache(root.root.path().to_path_buf());
        let (expected_files, sync_items) = get_deltas_with("dir1", vec![], &fs).await;
        let mut files_found = 0;
        for file in &temp_files(&root) {
            if file.contains("dir1") && file != "dir1" {
                files_found += 1;
                assert!(
                    expected_files.contains(file),
                    "{:?} Missing file: {} ",
                    expected_files,
                    file
                );
            }
        }
        assert_eq!(
            files_found,
            expected_files.len(),
            "Expected files: {:?}",
            expected_files
        );
        let (expected_files, sync_items) = get_deltas_with("dir1", sync_items, &fs).await;
        assert!(expected_files.is_empty());
        assert!(sync_items.is_empty());
    }
    async fn write_file(fs: &PortableFs, portable_path: &Path, data: &[u8]) -> FileStat {
        let modified = SystemTime::now();
        let stats = FileStat {
            size: data.len() as u64,
            mtime: format_system_time(modified),
            is_directory: false,
            sha256: Some(
                data.sha256_build()
                    .await
                    .unwrap()
                    .sha256_string()
                    .await
                    .unwrap(),
            ),
        };

        fs.write(portable_path, data, true, &stats).await.unwrap();
        stats
    }

    #[tokio::test]
    async fn test_write() {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let fs = PortableFs::with_cache(root.root.path().to_path_buf());

        let fpath: &[&str] = &["test_file.txt"];
        let portable_path = Path::try_from(fpath).unwrap();
        let data: &[u8] = b"Hello, world!";
        let stats = write_file(&fs, &portable_path, data).await;

        // Assert the file exists and contains the correct data
        let full_path = fs.as_abs_path(&portable_path);
        assert!(full_path.exists());
        let contents = tokio::fs::read(&full_path).await.unwrap();
        assert_eq!(contents, data);
        let metadata = tokio::fs::metadata(&full_path).await.unwrap();
        assert_eq!(metadata.len(), data.len() as u64);
        assert_eq!(
            parse_system_time(&stats.mtime).unwrap(),
            metadata.modified().unwrap()
        );
        assert_eq!(
            stats.sha256.as_ref().unwrap(),
            &data
                .sha256_build()
                .await
                .unwrap()
                .sha256_string()
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_delete_file() {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let fs = PortableFs::with_cache(root.root.path().to_path_buf());

        // Create a test file
        let portable_path = Path::try_from(&StdPath::new("test_file.txt").to_owned()).unwrap();
        let full_path = fs.as_abs_path(&portable_path);
        tokio::fs::write(&full_path, b"Hello, world!")
            .await
            .unwrap();

        // Delete the file
        fs.delete_file(&portable_path).await.unwrap();

        // Assert the file no longer exists
        assert!(!full_path.exists());
    }

    fn check_len(cache: &dyn Cache, expected_len: u64) {
        let len = cache.len();
        if len != expected_len {
            cache.dump_keys();
        }
        assert_eq!(len, expected_len);
    }

    #[tokio::test]
    async fn test_cache() {
        let root = TestRoot::new(std::thread::current().name()).await.unwrap();
        let fs = PortableFs::with_cache(root.root.path().to_path_buf());

        let mut cstats = CacheStats::default();
        check_len(fs.get_cache().as_ref(), 0);
        check_len(fs.get_cache().as_ref(), 0);
        assert_eq!(fs.get_cache().stats(), &cstats);

        let fpath: &[&str] = &["test_file.txt"];
        let portable_path = Path::try_from(fpath).unwrap();
        let data: &[u8] = b"Hello, world!";
        let stats = write_file(&fs, &portable_path, data).await;
        assert_eq!(fs.get_cache().stats(), &cstats);

        assert_eq!(fs.get_cache().get(&portable_path).unwrap(), &stats);
        check_len(fs.get_cache().as_ref(), 1);
        cstats.hits += 1;
        assert_eq!(fs.get_cache().stats(), &cstats);

        fs.delete_file(&portable_path).await.unwrap();
        check_len(fs.get_cache().as_ref(), 0);
        assert_eq!(fs.get_cache().stats(), &cstats);
        assert_eq!(fs.get_cache().get(&portable_path), None);
        cstats.misses += 1;
        assert_eq!(fs.get_cache().stats(), &cstats);

        let _ = fs
            .read_dir(&Path::try_from(&PathBuf::from("")).unwrap())
            .await;
        check_len(fs.get_cache().as_ref(), 4);
        cstats.misses += 4;
        assert_eq!(fs.get_cache().stats(), &cstats);

        let old_len = fs.get_cache().len();
        let _ = fs
            .read_dir_recurse(&Path::try_from(&PathBuf::from("")).unwrap())
            .await
            .unwrap();
        check_len(fs.get_cache().as_ref(), root.files.len() as u64);
        cstats.hits += old_len;
        cstats.misses += root.files.len() as u64 - old_len;
        assert_eq!(fs.get_cache().stats(), &cstats);
    }

    #[tokio::test]
    async fn test_filtering() {
        let mut pfs = PortableFs::without_cache("./".into());
        pfs.allow_extension("toml");
        let dir = pfs.read_dir(&Path::empty()).await.unwrap();
        let toml_files = ["Cargo.toml", "rustfmt.toml"];
        for entry in &dir.items {
            assert!(toml_files.contains(&entry.name.as_str()));
        }
        assert_eq!(dir.items.len(), toml_files.len());
    }
}
