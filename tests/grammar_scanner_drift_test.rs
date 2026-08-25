//! Drift gate: the tree-sitter grammar and the server's regex
//! scanner must agree on the structural constructs both understand.
//! Every corpus case is parsed twice — the EXPECTED tree text counts
//! grammar nodes, the INPUT goes through the scanner — and the counts
//! must match.  Extending the grammar (new corpus cases) without
//! teaching the scanner, or vice versa, trips this gate.

use opensips_lsp::analyze;

/// One corpus case: (title, input, expected-tree text).
fn parse_corpus(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        if !l.starts_with("====") {
            continue;
        }
        let title = lines.next().unwrap_or("").trim().to_string();
        // closing ==== of the header
        for l in lines.by_ref() {
            if l.starts_with("====") {
                break;
            }
        }
        let mut input = String::new();
        for l in lines.by_ref() {
            if l.starts_with("----") {
                break;
            }
            input.push_str(l);
            input.push('\n');
        }
        let mut expected = String::new();
        while let Some(l) = lines.peek() {
            if l.starts_with("====") {
                break;
            }
            expected.push_str(lines.next().unwrap());
            expected.push('\n');
        }
        out.push((title, input, expected));
    }
    out
}

fn count(hay: &str, node: &str) -> usize {
    hay.matches(&format!("({node}")).count()
}

#[test]
fn corpus_parser_detects_mismatches() {
    // self-test: the gate machinery must SEE a divergence
    let mini = "================\ncase\n================\n\nroute[a] { exit; }\n\n----------------\n\n(source_file (route_definition (route_kind) name: (identifier) (block (keyword_statement))) (route_definition))\n";
    let cases = parse_corpus(mini);
    assert_eq!(cases.len(), 1);
    let (_, input, expected) = &cases[0];
    assert_eq!(analyze::route_blocks(input).len(), 1);
    assert_eq!(count(expected, "route_definition"), 2, "mini fixture");
    assert_ne!(
        analyze::route_blocks(input).len(),
        count(expected, "route_definition"),
        "a rigged corpus MUST diverge — otherwise this gate tests nothing"
    );
}

#[test]
fn grammar_and_scanner_agree_on_the_corpus() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tree-sitter-opensips/test/corpus"
    );
    let mut cases = 0;
    let mut includes_seen = 0;
    for entry in std::fs::read_dir(dir).expect("corpus dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("corpus file");
        for (title, input, expected) in parse_corpus(&text) {
            cases += 1;
            assert_eq!(
                analyze::route_blocks(&input).len(),
                count(&expected, "route_definition"),
                "{title}: route definitions diverge\n--- input ---\n{input}"
            );
            assert_eq!(
                analyze::loaded_modules(&input).len(),
                count(&expected, "loadmodule"),
                "{title}: loadmodule count diverges\n--- input ---\n{input}"
            );
            assert_eq!(
                analyze::modparam_calls(&input).len(),
                count(&expected, "modparam"),
                "{title}: modparam count diverges\n--- input ---\n{input}"
            );
            // Includes were the one construct the two sides modelled
            // differently and nothing compared: the scanner followed
            // them — correctly, the real parser reads them — while
            // this grammar had no include rule at all.  An editor
            // driven by the grammar and a server driven by the
            // scanner then disagreed about what the file contains,
            // which for a feature built entirely on include
            // directives is the disagreement that matters most.
            assert_eq!(
                analyze::includes(&input).len(),
                count(&expected, "include"),
                "{title}: include count diverges\n--- input ---\n{input}"
            );
            includes_seen += count(&expected, "include");
        }
    }
    assert!(cases >= 8, "corpus went missing? saw {cases} cases");
    assert!(
        includes_seen >= 2,
        "the corpus must exercise includes — bare and `!!`-prefixed — or \
         that comparison is zero against zero: {includes_seen}"
    );
}
