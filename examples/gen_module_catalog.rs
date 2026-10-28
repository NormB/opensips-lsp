//! Regenerate the built-in module catalogue from a pinned source tree.
//!
//!     cargo run --example gen_module_catalog -- <tree> <version> > src/modules_builtin.json
//!
//! Vendored so that `loadmodule "` and module calls like `is_method`
//! complete before the user has configured anything.  A test asserts
//! the vendored file still equals a fresh harvest of the pinned tree,
//! so it cannot drift from the version it claims.
#![deny(clippy::unwrap_used)]

use opensips_lsp::catalog::BuiltinModules;
use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let tree = args
        .next()
        .expect("usage: gen_module_catalog <tree> <version>");
    let version = args
        .next()
        .expect("usage: gen_module_catalog <tree> <version>");
    let modules = opensips_lsp::catalog::harvest_tree(Path::new(&tree));
    let out = BuiltinModules { version, modules };
    println!(
        "{}",
        serde_json::to_string(&out).expect("a catalog of strings serializes to JSON")
    );
}
