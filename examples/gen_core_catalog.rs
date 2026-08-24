//! Regenerate the built-in core catalogue from a pinned source tree.
//!
//!     cargo run --example gen_core_catalog -- <tree> <version> > src/core_builtin.json
//!
//! The result is vendored so core-language completion works before the
//! user has configured anything.  A test asserts the vendored file
//! still equals a fresh harvest of the pinned tree, so it cannot drift.

fn main() {
    let mut args = std::env::args().skip(1);
    let tree = args
        .next()
        .expect("usage: gen_core_catalog <tree> <version>");
    let version = args
        .next()
        .expect("usage: gen_core_catalog <tree> <version>");
    let core = opensips_lsp::catalog::harvest_core(std::path::Path::new(&tree));
    let out = opensips_lsp::catalog::BuiltinCore { version, core };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
