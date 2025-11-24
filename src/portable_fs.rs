use std::num::NonZeroUsize;
use std::path::Path as StdPath;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;

#[cfg(not(target_arch = "wasm32"))]
use super::native::FsCache;
use crate::Path;
use crate::cache::Cache;
use crate::cache::NullCache;
use crate::filter::FilterSet;

/// Caching and filtering layers that sit above and below the `PortableFs`
#[derive(Clone)]
pub(crate) struct FsLayer {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) cache: Arc<Mutex<Box<dyn Cache>>>,
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
    pub(crate) base_dir: PathBuf,
    pub(crate) layer: Arc<FsLayer>,
}

impl PortableFs {
    fn with(base_dir: PathBuf, cache: Box<dyn Cache>) -> Self {
        PortableFs {
            base_dir,
            layer: Arc::new(FsLayer::new(cache, FilterSet::new())),
        }
    }

    /// creates portable fs with cache
    #[cfg(not(target_arch = "wasm32"))]
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
