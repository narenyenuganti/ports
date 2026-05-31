#!/usr/bin/env bash
# Tier 2 merge gate (full): fast gate + Swift build/test + explicit
# thermo-nuclear review skills. Run at the merge point before integrating.
set -euo pipefail

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/ports-target}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "==> Stage 1: fast gate"
"$SCRIPT_DIR/gate-fast.sh"

echo "==> Stage 1b: Swift build/test"
if [ -f "$REPO_ROOT/app/Package.swift" ]; then
  ( cd "$REPO_ROOT/app" && swift build && swift test )
else
  echo "    (skipped: app/Package.swift not present yet)"
fi

echo "==> Stage 2: thermo-nuclear-review (explicit, manual)"
echo "    Invoke the 'thermo-nuclear-review' skill on this branch's diff."
echo "    It COMPOSES WITH the superpowers review skills; do not skip."

echo "==> Stage 3: thermo-nuclear-code-quality-review (explicit, manual)"
echo "    Invoke the 'thermo-nuclear-code-quality-review' skill on this diff."
echo "    It COMPOSES WITH the superpowers review skills; do not skip."

echo "full gate OK (stages 2-3 require the manual thermo review skills above)"
