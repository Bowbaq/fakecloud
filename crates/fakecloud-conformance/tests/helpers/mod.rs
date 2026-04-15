//! Re-exports of `fakecloud_testkit` items used by the conformance suite.
//!
//! `TestServer`, including every per-service SDK client factory, lives in
//! `fakecloud_testkit` under the `sdk-clients` feature which this crate
//! enables in its `Cargo.toml`.

#![allow(dead_code, unused_imports)]

pub use fakecloud_testkit::TestServer;
