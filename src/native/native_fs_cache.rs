use std::num::NonZeroUsize;

use lru::LruCache;

use crate::FileStat;
use crate::Path;
use crate::cache::Cache;
use crate::cache::CacheStats;

pub(crate) struct FsCache {
    lru: LruCache<Path, FileStat>,
    stats: CacheStats,
}

impl FsCache {
    pub(crate) fn new(capacity: NonZeroUsize) -> Self {
        FsCache {
            lru: LruCache::new(capacity),
            stats: CacheStats::default(),
        }
    }
}

impl Cache for FsCache {
    fn get(&mut self, key: &Path) -> Option<&FileStat> {
        let ret = self.lru.get(key);
        if ret.is_some() {
            self.stats.hits += 1;
        } else {
            self.stats.misses += 1;
        }
        ret
    }

    fn put(&mut self, key: Path, value: FileStat) {
        self.lru.put(key, value);
    }

    #[cfg(test)]
    fn stats(&self) -> &CacheStats {
        &self.stats
    }

    #[cfg(test)]
    fn len(&self) -> u64 {
        self.lru.len() as u64
    }

    fn pop(&mut self, key: &Path) -> Option<FileStat> {
        self.lru.pop(key)
    }

    #[cfg(test)]
    fn dump_keys(&self) -> String {
        self.lru.iter().for_each(|(k, _v)| println!("\"{}\"", k));
        "".to_owned()
    }
}
