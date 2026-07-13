#![allow(missing_docs)]
//! Benchmarks for the portable-fs crate.

#[cfg(not(target_arch = "wasm32"))]
/// Benchmarks for the portable filesystem implementation.
mod portable_fs_bench;
#[cfg(not(target_arch = "wasm32"))]
use criterion::criterion_main;

#[cfg(not(target_arch = "wasm32"))]
criterion::criterion_group!(
    benches,
    portable_fs_bench::bench_read_dir,
    portable_fs_bench::bench_read_dir_recurse,
    portable_fs_bench::bench_read_dir_cached,
    portable_fs_bench::bench_path_append_to,
    portable_fs_bench::bench_path_try_from_pathbuf
);

#[cfg(not(target_arch = "wasm32"))]
criterion_main!(benches);

#[cfg(target_arch = "wasm32")]
fn main() {}
