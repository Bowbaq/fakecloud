//! Re-exports of `fakecloud_testkit` items used by the e2e suite, plus a
//! couple of crate-local helpers (`gunzip`) that don't belong in testkit.
//!
//! `TestServer`, including every per-service SDK client factory and the
//! `aws_cli` wrapper, lives in `fakecloud_testkit` under the `sdk-clients`
//! feature which this crate enables in its `Cargo.toml`.

#![allow(dead_code, unused_imports)]

use std::path::PathBuf;

pub use fakecloud_testkit::{data_path_for, run_until_exit, CliOutput, TestServer};

/// Decompress gzipped data.
pub fn gunzip(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result).unwrap();
    result
}

/// Re-exported for historical reasons. Some test helpers construct a
/// `PathBuf` from a `tempfile::TempDir` through this shim.
#[allow(dead_code)]
pub fn _path_buf_shim(p: PathBuf) -> PathBuf {
    p
}
