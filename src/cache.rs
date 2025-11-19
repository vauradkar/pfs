use std::num::NonZeroUsize;

use lru::LruCache;
#[cfg(feature = "poem")]
use poem_openapi::Object;
#[cfg(feature = "json_schema")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::FileStat;
use crate::Path;

#[cfg_attr(feature = "json_schema", derive(JsonSchema))]
#[cfg_attr(feature = "poem", derive(Object))]
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

pub(crate) trait Cache: Send {
    fn get(&mut self, key: &Path) -> Option<&FileStat>;

    fn put(&mut self, key: Path, value: FileStat);

    #[cfg(test)]
    fn stats(&self) -> &CacheStats;

    #[cfg(test)]
    fn len(&self) -> u64;

    fn pop(&mut self, key: &Path) -> Option<FileStat>;

    #[cfg(test)]
    fn dump_keys(&self) -> String;
}

pub(crate) struct NullCache {
    #[allow(dead_code)]
    stats: CacheStats,
}

impl NullCache {
    pub(crate) fn new(_capacity: NonZeroUsize) -> Self {
        Self {
            stats: CacheStats::default(),
        }
    }
}

impl Cache for NullCache {
    fn get(&mut self, _key: &Path) -> Option<&FileStat> {
        None
    }

    fn put(&mut self, _key: Path, _value: FileStat) {}

    #[cfg(test)]
    fn stats(&self) -> &CacheStats {
        &self.stats
    }

    #[cfg(test)]
    fn len(&self) -> u64 {
        0
    }

    fn pop(&mut self, _key: &Path) -> Option<FileStat> {
        None
    }

    #[cfg(test)]
    fn dump_keys(&self) -> String {
        "".to_owned()
    }
}

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
