//! Emits the `SERVICES` allow-list as a JSON array of strings for CI matrix
//! fan-out. Matches the shape consumed by the `tfacc` workflow — one
//! GitHub Actions runner per service.
//!
//! Kept as a tiny binary (not a cargo feature) so CI can invoke it without
//! building the rest of the workspace first.

use fakecloud_tfacc::SERVICES;

fn main() {
    let names: Vec<&str> = SERVICES.iter().map(|s| s.name).collect();
    let json = serde_json_mini(&names);
    println!("{json}");
}

/// Tiny hand-rolled JSON encoder — avoids pulling serde_json into this
/// binary's dep graph just to emit `["a","b"]`.
fn serde_json_mini(items: &[&str]) -> String {
    let mut out = String::from("[");
    for (i, name) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(name);
        out.push('"');
    }
    out.push(']');
    out
}
