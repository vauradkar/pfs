//! A portable, crass-platform, serializable reprentation of file system
//!
//! A typical use for this crate is server sending file information to a
//! client.
//!
//! ```rust
//! # tokio_test::block_on(async {
//! # use pfs::PortableFs;
//! # use pfs::Path;
//! let mut pfs = PortableFs::without_cache("./".into());
//! pfs.allow_extension("toml");
//! let dir = pfs.read_dir(&Path::empty()).await.unwrap();
//! let toml_files = ["Cargo.toml", "rustfmt.toml"];
//! for entry in &dir.items {
//!     assert!(toml_files.contains(&entry.name.as_str()));
//! }
//! assert_eq!(dir.items.len(), toml_files.len());
//! println!("{}", serde_json::to_string_pretty(&dir).unwrap());
//! # })
//! ```
//!
//! The output might look like
//! ```json
//! {
//!   "current_path": {
//!     "components": []
//!   },
//!   "items": [
//!     {
//!       "name": "Cargo.toml",
//!       "stats": {
//!         "size": 1581,
//!         "mtime": "2025-11-20T00:35:58.153Z",
//!         "is_directory": false,
//!         "sha256": "6e4c7f34b5956bbf053ae1f14b70c5cf02a748a1a6834c4bb915b1bc26ea3051"
//!       }
//!     },
//!     {
//!       "name": "rustfmt.toml",
//!       "stats": {
//!         "size": 102,
//!         "mtime": "2025-11-18T04:34:31.565Z",
//!         "is_directory": false,
//!         "sha256": "632059ea2fe5e8b96994e5529a96a981a68bf62cf1728c6f75be7f83439805b5"
//!       }
//!     }
//!   ]
//! }
//! ```

mod cache;
mod dir;
mod dir_list;
mod errors;
mod file;
mod filter;
pub mod hash;
mod native_fs;
mod path;
mod portable_fs;
pub mod utils;

pub use dir::Directory;
pub use dir::DirectoryEntry;
pub use dir_list::RecursiveDirList;
pub use errors::Error;
pub use file::FileInfo;
pub use file::FileNode;
pub use file::FileStat;
pub use path::Path;
pub use portable_fs::PortableFs;

#[cfg(feature = "test_utils")]
pub(crate) mod test_utils;
#[cfg(feature = "test_utils")]
pub use test_utils::TestRoot;
