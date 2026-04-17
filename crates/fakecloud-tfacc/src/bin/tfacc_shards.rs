//! Emits the `SHARDS` matrix as JSON for CI fan-out.
//!
//! Each element is `{"name":"...","service":"..."}` — the workflow uses
//! `name` as the job display name and `service` is kept for context.
//! The matching `#[tokio::test]` in `tests/acc.rs` picks up the shard
//! metadata via `SHARDS` too, so this binary only needs to emit enough
//! for the GHA matrix to fan out.

use fakecloud_tfacc::SHARDS;

fn main() {
    let mut out = String::from("[");
    for (i, shard) in SHARDS.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // test_fn matches the `#[tokio::test]` in `tests/acc.rs` — shard
        // names use dashes, Rust function names use underscores.
        let test_fn = format!("{}_acceptance", shard.name.replace('-', "_"));
        out.push_str(&format!(
            r#"{{"name":"{}","service":"{}","test_fn":"{}"}}"#,
            shard.name, shard.service, test_fn
        ));
    }
    out.push(']');
    println!("{out}");
}
