//! Any text as a module README.md, as the harvester reads every module's
//! documentation from an OpenSIPS source tree. A README that is read gives
//! only named parameters and functions.
#![no_main]
#![deny(clippy::unwrap_used)]

use libfuzzer_sys::fuzz_target;
use opensips_lsp::catalog;

fuzz_target!(|md: &str| {
    if let Ok(doc) = catalog::parse_readme_md("fuzzed", md) {
        for item in doc.params.iter().chain(&doc.functions) {
            assert!(!item.name.is_empty(), "an item with no name");
        }
    }
});
