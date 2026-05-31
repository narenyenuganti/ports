#!/usr/bin/env bash
# Tier 1 merge gate (fast): formatting, lints, and tests.
# Run on every commit and before every push.
set -euo pipefail

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/ports-target}"

echo "==> [1/3] cargo fmt --all --check"
cargo fmt --all --check

echo "==> [2/3] cargo clippy --all-targets"
cargo clippy --all-targets

echo "==> [3/3] cargo test --all"
cargo test --all

echo "fast gate OK"
