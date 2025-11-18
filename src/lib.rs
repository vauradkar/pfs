//! A portable, crass-platform, serializable reprentation of file system

mod errors;
mod file;
mod hash;
mod path;
pub mod utils;

pub use errors::Error;
pub use file::FileInfo;
pub use file::FileStat;
pub use path::Path;
