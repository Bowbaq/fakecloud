#!/usr/bin/env bash
# Reproduce the CI `test` job's coverage run locally.
# Requires: cargo-llvm-cov (install with `cargo install cargo-llvm-cov`).
#
# Scope mirrors .github/workflows/ci.yml exactly: workspace minus the
# end-to-end / acceptance / parity crates, plus the conformance crate's
# library tests. Extra args are forwarded to `cargo llvm-cov report`
# (e.g. `scripts/coverage.sh --lcov --output-path lcov.info`).
set -euo pipefail

cargo llvm-cov clean --workspace
cargo llvm-cov --no-report --workspace \
  --exclude fakecloud-e2e \
  --exclude fakecloud-conformance \
  --exclude fakecloud-tfacc \
  --exclude fakecloud-parity
cargo llvm-cov --no-report -p fakecloud-conformance --lib
cargo llvm-cov report "$@"
