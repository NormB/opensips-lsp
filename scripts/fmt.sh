#!/usr/bin/env bash
# Formats the Rust code with the options in rustfmt.toml. One script, so CI and
# a developer check and fix with the same rustfmt and the same options.
#
#   scripts/fmt.sh check       fail, with the diff, if any file is not formatted
#   scripts/fmt.sh fix         format every file in place
#   scripts/fmt.sh toolchain   the toolchain this script runs rustfmt from
#
# rustfmt.toml follows the Google Rust Style Guide
# (https://google.github.io/styleguide/rustguide.html). Stable rustfmt
# 1.10.0 ignores format_macro_matchers and imports_granularity with a
# warning and leaves hex literals as they are, so rustfmt runs from the
# nightly named below, pinned by date. Only rustfmt runs from
# it: the crate builds and is linted with the stable toolchain and uses no
# unstable language feature. Pick a new date at least two weeks old.
#
# The fuzz package is a workspace of its own, so it is formatted by name.
set -euo pipefail
TOOLCHAIN=nightly-2026-10-06
root="$(cd "$(dirname "$0")/.." && pwd)"

# Both packages run even when the first fails, so one check lists every file.
fmt() {
  local rc=0
  cargo "+$TOOLCHAIN" fmt --manifest-path "$root/Cargo.toml" "$@" || rc=1
  cargo "+$TOOLCHAIN" fmt --manifest-path "$root/fuzz/Cargo.toml" "$@" || rc=1
  return "$rc"
}

case "${1:-}" in
  check) fmt --check ;;
  fix) fmt ;;
  toolchain) echo "$TOOLCHAIN" ;;
  *)
    echo "usage: scripts/fmt.sh check|fix|toolchain" >&2
    exit 2
    ;;
esac
