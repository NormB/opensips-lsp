//! Any text as a cfg document, through the scanner every feature reads
//! (`analyze`), the outline and folds, and the formatter. Every position
//! it reports lies inside the document, no block ends before it starts,
//! and formatting a formatted document changes nothing.
#![no_main]

use libfuzzer_sys::fuzz_target;
use opensips_lsp::{analyze, format, outline};

fuzz_target!(|text: &str| {
    // A position may sit on the line after the last newline.
    let lines = text.split('\n').count() as u32;
    let inside = |line: u32| line < lines;

    for found in [
        analyze::loaded_modules(text),
        analyze::includes(text),
        analyze::route_defs(text),
        analyze::route_refs(text),
    ] {
        for at in found {
            assert!(inside(at.line), "line {} of {lines}", at.line);
        }
    }
    for (at, _) in analyze::armed_route_refs(text) {
        assert!(inside(at.line));
    }
    for block in analyze::route_blocks(text) {
        assert!(inside(block.end_line));
        assert!((block.end_line, block.end_col) >= (block.line, block.col));
    }
    for fold in outline::folds(text) {
        assert!(inside(fold.end_line));
        assert!(fold.end_line >= fold.start_line);
    }
    let _ = analyze::calls(text);
    let _ = analyze::modparam_calls(text);
    let _ = analyze::pvars(text);
    let _ = outline::outline(text);

    for opts in [
        format::Options::default(),
        format::Options {
            insert_spaces: true,
            tab_size: 2,
        },
    ] {
        let once = format::format(text, &opts);
        assert_eq!(
            format::format(&once, &opts),
            once,
            "formatting is not stable"
        );
    }
});
