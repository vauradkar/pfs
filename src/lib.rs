//! A portable, crass-platform, serializable reprentation of file system

mod dir;
mod dir_list;
mod errors;
mod file;
pub mod hash;
mod path;
pub mod utils;

pub use dir::Directory;
pub use dir::DirectoryEntry;
pub use dir_list::RecursiveDirList;
pub use errors::Error;
pub use file::FileInfo;
pub use file::FileNode;
pub use file::FileStat;
pub use path::Path;

#[cfg(feature = "test_utils")]
pub(crate) mod test_utils;
#[cfg(feature = "test_utils")]
pub use test_utils::TestRoot;
