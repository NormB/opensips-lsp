#!/bin/bash -eu
# Builds every fuzz target for ClusterFuzzLite. It runs inside the builder
# image, which sets $SRC, $OUT and the sanitizer flags.
cd "$SRC/opensips-lsp/fuzz"

# The image sets RUSTUP_TOOLCHAIN to its own nightly, which overrides
# fuzz/rust-toolchain.toml; use the one that file names.
RUSTUP_TOOLCHAIN="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"
export RUSTUP_TOOLCHAIN

cargo fuzz build -O --debug-assertions
release="$SRC/opensips-lsp/fuzz/target/$(rustc -vV | sed -n 's|^host: ||p')/release"

# The targets come from fuzz/Cargo.toml, as in CI; the seeds are the ones
# scripts/fuzz.sh reads.
built=0
for target in $(cargo fuzz list); do
  cp "$release/$target" "$OUT/"
  built=$((built + 1))
  seeds=("$SRC/opensips-lsp/fuzz/seeds/$target")
  case "$target" in
    cfg) seeds+=("$SRC/opensips-lsp/tests/grammar" "$SRC/opensips-lsp/tree-sitter-opensips/test/highlight") ;;
  esac
  files=()
  for dir in "${seeds[@]}"; do
    files+=("$dir"/*)
  done
  zip -j -q "$OUT/${target}_seed_corpus.zip" "${files[@]}"
done
if [ "$built" -eq 0 ]; then
  echo "build.sh: cargo fuzz list gave no targets" >&2
  exit 1
fi
echo "build.sh: built $built fuzz targets"
