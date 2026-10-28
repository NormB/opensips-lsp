#!/usr/bin/env bash
# The local gate: the checks CI runs, run here first so that a failure costs
# seconds instead of a CI round. The hooks run it; so can anyone.
#
#   scripts/gate.sh commit   rustfmt (the pinned nightly), clippy on both
#                            packages with warnings as errors, and every test
#   scripts/gate.sh push     rustdoc with warnings as errors, the MSRV build,
#                            the VS Code client's compile and tests, and
#                            cargo audit
#
# The lints come from the [lints] tables in Cargo.toml and fuzz/Cargo.toml,
# so clippy here and in CI denies the same ones.
#
# The tests need the proof environment (a skipped test is a failed test in
# this repo). It is read from env.sh in the repository's git directory when
# that file exists; write it once with
#   scripts/proof-env.sh > "$(git rev-parse --git-common-dir)/env.sh"
# Otherwise the variables must already be set.
#
# Left to CI, because each needs a download or minutes of its own: the
# tree-sitter grammar build and its sanitizer run, the fuzz targets, and
# packaging the client with vsce.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

step() {
  local name="$1"
  shift
  echo "gate: $name" >&2
  if ! "$@"; then
    echo "gate: refused: $name failed" >&2
    exit 1
  fi
}

commit_gate() {
  local env_file
  env_file="$(git rev-parse --path-format=absolute --git-common-dir)/env.sh"
  if [ -f "$env_file" ]; then
    # shellcheck disable=SC1090
    . "$env_file"
  fi
  step "rustfmt" scripts/fmt.sh check
  step "clippy" cargo clippy --all-targets --quiet -- -D warnings
  step "clippy (fuzz)" cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --quiet -- -D warnings
  step "tests" cargo test --all-targets --quiet
}

# The installed toolchain for the rust-version in Cargo.toml: that version
# itself, or its newest patch release.
msrv_toolchain() {
  local want
  want="$(grep -m1 '^rust-version' Cargo.toml | cut -d'"' -f2)"
  rustup toolchain list | cut -d' ' -f1 | sed -E 's/-[a-z].*$//' |
    grep -E "^${want//./\\.}(\.[0-9]+)?$" | sort -V | tail -1
}

push_gate() {
  step "rustdoc" env RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --quiet
  local msrv
  msrv="$(msrv_toolchain || true)"
  if [ -z "$msrv" ]; then
    echo "gate: refused: no toolchain for the MSRV in Cargo.toml; rustup toolchain install $(grep -m1 '^rust-version' Cargo.toml | cut -d'"' -f2)" >&2
    exit 1
  fi
  step "MSRV ($msrv)" cargo "+$msrv" check --all-targets --quiet
  if [ ! -d client/node_modules ]; then
    echo "gate: refused: client/node_modules is missing; run npm ci in client/" >&2
    exit 1
  fi
  # The behavior suites load the compiled client, so compile first.
  step "client compile" npm --prefix client run --silent compile
  step "client tests" npm --prefix client test --silent
  step "cargo audit" cargo audit --deny warnings --quiet
}

case "${1:-}" in
  commit) commit_gate ;;
  push) push_gate ;;
  *)
    echo "usage: scripts/gate.sh commit|push" >&2
    exit 2
    ;;
esac
