//! Any text as a module's DocBook admin guide (`doc/*_admin.xml`), the
//! harvester's other source. A guide that is read gives only named
//! parameters and functions.
#![no_main]
#![deny(clippy::unwrap_used)]

use libfuzzer_sys::fuzz_target;
use opensips_lsp::catalog;

fuzz_target!(|xml: &str| {
    if let Ok(doc) = catalog::parse_admin_xml("fuzzed", xml) {
        for item in doc.params.iter().chain(&doc.functions) {
            assert!(!item.name.is_empty(), "an item with no name");
        }
    }
});
