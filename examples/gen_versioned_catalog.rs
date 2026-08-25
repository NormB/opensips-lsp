//! Regenerate the built-in module catalogue from several pinned trees.
//!
//!     cargo run --example gen_versioned_catalog -- \
//!         3.5.9=/path/to/3.5.9 3.6.8=/path/to/3.6.8 4.0.1=/path/to/4.0.1 \
//!         > src/modules_builtin.json
//!
//! Releases must be given oldest first: the first becomes the base and
//! each later one becomes a delta against the release before it. The
//! trees need only to be checked out — nothing is built, because the
//! catalogue is harvested from source files alone.
//!
//! `scripts/proof-env.sh` provisions exactly these trees and exports
//! them as `OPENSIPS_LSP_TEST_TREES` in this same `<version>=<path>`
//! form, so the vendored file and the suite that checks it derive from
//! one list.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!(
            "usage: gen_versioned_catalog <version>=<tree> <version>=<tree> [...]  (oldest first)"
        );
        std::process::exit(2);
    }

    let mut harvests: Vec<(String, Vec<opensips_lsp::catalog::ModuleDoc>)> = Vec::new();
    for arg in &args {
        let Some((version, tree)) = arg.split_once('=') else {
            eprintln!("expected <version>=<tree>, got '{arg}'");
            std::process::exit(2);
        };
        let mut modules = opensips_lsp::catalog::harvest_tree(std::path::Path::new(tree));
        if modules.is_empty() {
            eprintln!("'{tree}' yields no module documentation");
            std::process::exit(2);
        }
        // the delta is only meaningful over a canonical order
        opensips_lsp::catalog::canonicalize(&mut modules);
        eprintln!(
            "{version}: {} modules, {} parameters",
            modules.len(),
            modules.iter().map(|m| m.params.len()).sum::<usize>()
        );
        harvests.push((version.to_string(), modules));
    }

    let deltas = harvests
        .windows(2)
        .map(|pair| opensips_lsp::catalog::diff_catalogues(&pair[0].1, &pair[1].1, &pair[1].0))
        .collect();
    let out = opensips_lsp::catalog::VersionedModules {
        base: opensips_lsp::catalog::BuiltinModules {
            version: harvests[0].0.clone(),
            modules: harvests[0].1.clone(),
        },
        deltas,
    };
    println!("{}", serde_json::to_string(&out).unwrap());
}
