//! Benchmarks for the portable_fs crate.
//! This file contains benchmarks for the portable_fs crate, which provides a
//! portable filesystem abstraction.

use std::hint::black_box;
use std::path::Path as StdPath;
use std::path::PathBuf;

use criterion::Criterion;
use pfs::Path;
use pfs::PortableFs;
use tempfile::tempdir;
use tokio::runtime::Runtime;

pub(crate) fn bench_read_dir(c: &mut Criterion) {
    let _rt = Runtime::new().unwrap();
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    // create a simple directory structure
    std::fs::create_dir_all(root.join("subdir")).unwrap();
    std::fs::write(root.join("file1.txt"), b"hello").unwrap();
    std::fs::write(root.join("subdir").join("file2.txt"), b"world").unwrap();

    let rt = Runtime::new().unwrap();
    let fs = PortableFs::with_cache(root.clone());
    let portable_path = Path::empty();

    c.bench_function("read_dir", |b| {
        b.iter(|| {
            rt.block_on(fs.read_dir(&portable_path, true)).unwrap();
        });
    });
}

pub(crate) fn bench_read_dir_recurse(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    // create a directory tree
    std::fs::create_dir_all(root.join("subdir/nested")).unwrap();
    std::fs::write(root.join("file1.txt"), b"hello").unwrap();
    std::fs::write(root.join("subdir").join("file2.txt"), b"world").unwrap();
    std::fs::write(root.join("subdir/nested").join("file3.txt"), b"foo").unwrap();

    let fs = PortableFs::with_cache(root.clone());
    let portable_path = Path::empty();

    c.bench_function("read_dir_recurse", |b| {
        b.iter(|| {
            rt.block_on(fs.read_dir_recurse(&portable_path, true)).unwrap();
        });
    });
}

pub(crate) fn bench_read_dir_cached(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();

    // create a simple directory structure and warm the cache
    std::fs::create_dir_all(root.join("subdir")).unwrap();
    std::fs::write(root.join("file1.txt"), b"hello").unwrap();
    std::fs::write(root.join("subdir").join("file2.txt"), b"world").unwrap();

    let fs = PortableFs::with_cache(root.clone());
    let portable_path = Path::empty();
    rt.block_on(fs.read_dir(&portable_path, true)).unwrap();

    c.bench_function("read_dir_cached", |b| {
        b.iter(|| {
            rt.block_on(fs.read_dir(&portable_path, true)).unwrap();
        });
    });
}

pub(crate) fn bench_path_append_to(c: &mut Criterion) {
    let base = StdPath::new("/tmp");
    let path_components: Vec<String> = (0..10).map(|i| format!("comp{}", i)).collect();
    let p = Path::try_from(
        path_components
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .as_slice(),
    )
    .unwrap();

    c.bench_function("path_append_to", |b| {
        b.iter(|| {
            let result = p.append_to(base);
            black_box(result);
        });
    });
}

pub(crate) fn bench_path_try_from_pathbuf(c: &mut Criterion) {
    let mut path = PathBuf::from("/tmp");

    // Generate a random number of components and create a path with multiple
    // components.
    let num_components = fastrand::usize(1..=10);
    for _ in 0..num_components {
        path.push(format!("comp{}", fastrand::usize(1..=100)));
    }

    path.push("file.txt");

    c.bench_function("path_try_from_pathbuf", |b| {
        b.iter(|| {
            let result = Path::try_from(&path).unwrap();
            black_box(result);
        });
    });
}
