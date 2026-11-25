# The image ClusterFuzzLite builds the fuzz targets in. The base image is
# pinned by digest; its own nightly Rust is older than this crate
# needs, so the nightly named in fuzz/rust-toolchain.toml is installed.
FROM gcr.io/oss-fuzz-base/base-builder-rust:v1@sha256:d345218878c78feed410f0ad57268617a368912b224f844bbdb71c54c0b9955a
COPY fuzz/rust-toolchain.toml /tmp/rust-toolchain.toml
RUN channel="$(sed -n 's/^channel = "\(.*\)"$/\1/p' /tmp/rust-toolchain.toml)" \
 && test -n "$channel" \
 && rustup toolchain install "$channel" --profile minimal --component rust-src \
 && rm /tmp/rust-toolchain.toml
# The tree as ClusterFuzzLite was given it, not a fresh clone, so a pull
# request is fuzzed as it stands.
COPY . $SRC/opensips-lsp
COPY .clusterfuzzlite/build.sh $SRC/build.sh
WORKDIR $SRC/opensips-lsp
