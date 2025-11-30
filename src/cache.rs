use std::num::NonZeroUsize;

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
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
