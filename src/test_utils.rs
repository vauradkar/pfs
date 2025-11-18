use std::collections::BTreeMap;
use std::fs;
use std::fs::create_dir_all;
use std::path::Path as StdPath;
use std::path::PathBuf;

use async_walkdir::WalkDir;
use cross_check::get_recursive_files;
use futures_lite::StreamExt;
use similar::ChangeTag;
use similar::TextDiff;
use tempdir::TempDir;

use crate::Directory;
use crate::Error;
use crate::FileInfo;
use crate::FileNode;
use crate::FileStat;

// File paths and optional contents to create in the temporary test
pub(crate) static TEMP_FILES: &[(&str, &str, bool)] = &[
    ("file1.txt", "", false),
    ("file2.txt", "", false),
    ("dir1", "", true),
    ("dir1/file3.txt", "", false),
    ("dir1/dir2", "", true),
    ("dir1/dir2/file4.txt", "", false),
    ("dir1/dir2/dir_empty1", "", true),
    ("dir3", "", true),
    ("dir3/file6.txt", "", false),
];

/// Utility structure for managing a temporary test directory and its files.
#[derive(Debug)]
pub struct TestRoot {
    /// Root of the temporary test directory.
    pub root: TempDir,
    /// Set of file paths created in the test root.
    pub files: BTreeMap<PathBuf, FileNode>,

    save_path: Option<PathBuf>,
}

impl TestRoot {
    /// Creates a new `TestRoot` instance with a temporary directory.
    pub async fn new(save_path: Option<&str>) -> Result<Self, Error> {
        let root = TempDir::new("").map_err(|e| Error::Create {
            what: "temporary directory".into(),
            how: e.to_string(),
        })?;
        let mut ret = Self {
            root,
            files: BTreeMap::new(),
            save_path: save_path.map(|p| StdPath::new("/tmp/").join(p)),
        };
        for (relative_path, contents, is_dir) in TEMP_FILES {
            let dir = if *is_dir {
                StdPath::new(relative_path)
            } else {
                StdPath::new(relative_path).parent().unwrap()
            };
            create_dir_all(ret.root.path().join(dir)).map_err(|e| Error::Create {
                what: format!("directory {}", dir.display()),
                how: e.to_string(),
            })?;
            if !*is_dir {
                ret.create_file(relative_path, Some(contents))
                    .await
                    .unwrap();
            }
        }
        ret.reload_files().await?;
        Ok(ret)
    }

    /// Creates a new file with the specified relative path and content in the
    /// temporary test directory.
    pub async fn create_file(
        &mut self,
        relative_path: &str,
        content: Option<&str>,
    ) -> Result<(), std::io::Error> {
        let full_path = self.root.path().join(relative_path);
        if let Some(parent) = full_path.parent() {
            create_dir_all(parent)?;
        }
        if let Some(content) = content {
            std::fs::write(&full_path, content)?;
        }
        let stat = FileStat::from_path(&full_path).await.unwrap();
        self.files.insert(
            relative_path.into(),
            FileNode::new(stat, content.unwrap_or("").as_bytes().to_vec()),
        );

        let mut parent = StdPath::new(relative_path);
        while let Some(p) = parent.parent() {
            let dir_path = self.root.path().join(p);
            let dir_stat = FileStat::from_path(&dir_path).await.unwrap();
            self.files
                .insert(p.to_path_buf(), (dir_stat, vec![]).into());
            parent = p;
        }

        Ok(())
    }

    async fn get_contents(&self, path: &StdPath, stats: &FileStat) -> Result<Vec<u8>, Error> {
        if stats.is_directory {
            Ok(vec![])
        } else {
            fs::read(self.root.path().join(path)).map_err(|e| Error::Read {
                what: format!("{}", path.display()),
                how: e.to_string(),
            })
        }
    }

    async fn get_insertable(&self, path: &StdPath) -> Result<(PathBuf, FileNode), Error> {
        let stats = FileStat::from_path(path).await?;
        let relative_path = path
            .strip_prefix(self.root.path())
            .map_err(|e| Error::Read {
                what: "strip_prefix".into(),
                how: e.to_string(),
            })?;

        let contents = self.get_contents(relative_path, &stats).await.unwrap();
        Ok((relative_path.to_owned(), (stats, contents).into()))
    }

    async fn reload_files(&mut self) -> Result<(), Error> {
        let mut new_files: BTreeMap<PathBuf, FileNode> = BTreeMap::new();
        let mut entries = WalkDir::new(self.root.path());
        loop {
            match entries.next().await {
                Some(Ok(entry)) => {
                    let (p, s) = self.get_insertable(&entry.path()).await?;
                    new_files.insert(p, s);
                }
                Some(Err(e)) => {
                    return Err(Error::Read {
                        what: "reading directory entry".into(),
                        how: e.to_string(),
                    });
                }
                None => break,
            }
        }
        self.files = new_files;
        Ok(())
    }

    /// Returns error if they are this directory and items are not synced.
    pub async fn are_synced(&self, items: &[FileInfo]) -> Result<(), Error> {
        let mut files: BTreeMap<PathBuf, FileNode> = BTreeMap::new();
        for item in items {
            let item_path = PathBuf::from(&item.path);
            files.insert(
                item_path.clone(),
                (
                    item.stats.clone(),
                    self.get_contents(&item_path, &item.stats).await?,
                )
                    .into(),
            );
        }

        println!("on_disk files: {:#?}", self.files);
        println!("incoming files: {files:#?}");
        if files != self.files {
            let (more, _more_name, less, less_name) = if self.files.len() > files.len() {
                (&self.files, "on_disk", &files, "incoming")
            } else {
                (&files, "incoming", &self.files, "on_disk")
            };

            for (path, rep) in more {
                match less.get(path) {
                    Some(other_stat) => {
                        if rep != other_stat && !rep.stats.is_directory {
                            return Err(Error::Sync {
                                what: format!("File stats do not match for {}", path.display()),
                                how: format!("expected: {rep:?}, found: {other_stat:?}"),
                            });
                        }
                    }
                    None => {
                        return Err(Error::Sync {
                            what: format!("File missing: {} in {}", path.display(), less_name),
                            how: "File not found in synced items".to_string(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Verify that all entries in `dir` match the files recorded in this
    /// TestRoot; panics if any entry's stats differ or are missing.
    pub fn match_entries(&self, dir: &Directory) {
        let dir_path = PathBuf::from(&dir.current_path);
        for item in &dir.items {
            let path = dir_path.join(&item.name);
            println!("Matching entry: {}", path.display());
            let rep = self.files.get(&path).unwrap();
            assert_eq!(item.stats, rep.stats);
        }
    }

    fn copy_dir_all(src: impl AsRef<StdPath>, dst: impl AsRef<StdPath>) -> Result<(), Error> {
        create_dir_all(&dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            if ty.is_dir() {
                Self::copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name())).unwrap();
            } else {
                fs::copy(entry.path(), dst.as_ref().join(entry.file_name())).unwrap();
            }
        }
        Ok(())
    }

    /// Returns none if the two directories are identical, or a string
    /// containing a diff of their contents if they differ.
    pub fn compare(&self, other_dir: &StdPath) -> Result<Option<String>, Error> {
        let mut other_files = get_recursive_files(other_dir)?;
        other_files.sort();
        let mut self_files = get_recursive_files(self.root.path())?;
        self_files.sort();
        let self_buf = self_files.join("\n");
        let other_buf = other_files.join("\n");

        let diff = TextDiff::from_lines(&self_buf, &other_buf);
        let mut diffs = String::new();
        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
                ChangeTag::Equal => continue,
            };

            diffs.push_str(&format!("{}{}", sign, change));
        }
        if diffs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(diffs))
        }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        if let Some(save_path) = &self.save_path {
            let _ = Self::copy_dir_all(self.root.path(), save_path);
            println!("TestRoot preserved at {}", save_path.to_string_lossy());
        }
    }
}

// The functions in the mod are intentionally written with an
// alternative approach to ensure that the main logic of accessing
// fs is not broken.
mod cross_check {
    use std::fs;
    use std::io::Read;
    use std::path::Path as StdPath;
    use std::time::SystemTime;

    use sha2::Digest;
    use sha2::Sha256;

    use crate::Error;

    pub(super) fn get_recursive_files(dir_path: &StdPath) -> Result<Vec<String>, Error> {
        let mut ret = vec![];
        visit_dirs(dir_path, dir_path, &mut ret)?;
        Ok(ret)
    }

    fn visit_dirs(
        base_path: &StdPath,
        current_path: &StdPath,
        out: &mut Vec<String>,
    ) -> Result<(), Error> {
        if current_path.is_dir() {
            for entry in fs::read_dir(current_path).map_err(|e| Error::Read {
                what: current_path.display().to_string(),
                how: e.to_string(),
            })? {
                let entry = entry.map_err(|e| Error::Read {
                    what: current_path.display().to_string(),
                    how: e.to_string(),
                })?;
                let path = entry.path();

                if path.is_dir() {
                    visit_dirs(base_path, &path, out)?;
                } else {
                    out.push(get_file_info(base_path, &path)?);
                }
            }
        }
        Ok(())
    }

    fn get_file_info(base_path: &StdPath, file_path: &StdPath) -> Result<String, Error> {
        let metadata = fs::metadata(file_path).map_err(|e| Error::Read {
            what: file_path.display().to_string(),
            how: e.to_string(),
        })?;

        let rel_path = file_path
            .strip_prefix(base_path)
            .unwrap_or(file_path)
            .display();

        let size = metadata.len();

        let mtime = metadata
            .modified()
            .map_err(|e| Error::Read {
                what: file_path.display().to_string(),
                how: e.to_string(),
            })?
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let sha256 = if metadata.is_file() {
            calculate_sha256(file_path)?
        } else {
            String::from("N/A")
        };

        Ok(format!(
            "{}\t{}\t{}\t{}\t{}",
            rel_path,
            size,
            mtime,
            sha256,
            if metadata.is_dir() { "DIR" } else { "FILE" }
        ))
    }

    fn calculate_sha256(path: &StdPath) -> Result<String, Error> {
        let mut file = fs::File::open(path).map_err(|e| Error::Read {
            what: path.display().to_string(),
            how: e.to_string(),
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let n = file.read(&mut buffer).map_err(|e| Error::Read {
                what: path.display().to_string(),
                how: e.to_string(),
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}
