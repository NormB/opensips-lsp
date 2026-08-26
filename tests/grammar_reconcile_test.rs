//! Names the grammar accepts and the manual never mentions.
//!
//! GIVEN the lexer accepts `workdir` and `wdir` for the same setting,
//! and the manual documents only one of them,
//! WHEN a reader writes the other,
//! THEN it must still hover — it is the same setting.
//!
//! And where the manual documents NEITHER spelling, the setting is
//! still real: `memdump = 3` and `memlog = 2` are accepted by the
//! grammar and appear in no manual page, as are the script calls
//! `xdbg(...)` and `error(...)`. Offering nothing for them says they
//! do not exist. Offering them with a note says what is true.
//!
//! Two lexer shapes carry different meanings and must not be
//! confused. Alternatives INSIDE one group are ALIASES —
//! `"workdir"|"wdir"` is one setting spelled two ways. Separate
//! groups are PARTS of one spelling —
//! `("allow"|"ALLOW")[-_]("proxy"|"PROXY")` is `allow_proxy`, and
//! reading it as aliases invents two settings nobody can write.

mod common;

use opensips_lsp::catalog::lexer_spellings;

#[test]
fn alternatives_in_one_group_are_aliases() {
    assert_eq!(
        lexer_spellings("\"workdir\"|\"wdir\""),
        vec!["workdir", "wdir"],
        "one setting, two spellings"
    );
}

#[test]
fn separate_groups_are_parts_of_one_spelling() {
    assert_eq!(
        lexer_spellings("(\"allow\"|\"ALLOW\")[-_](\"proxy\"|\"PROXY\")"),
        vec!["allow_proxy"],
        "one spelling in two parts — not two settings"
    );
}

#[test]
fn a_bare_word_is_its_own_spelling() {
    assert_eq!(lexer_spellings("error"), vec!["error"]);
    assert_eq!(lexer_spellings("\"xdbg\""), vec!["xdbg"]);
}

#[test]
fn upper_case_alternatives_are_not_spellings_of_their_own() {
    assert_eq!(
        lexer_spellings("\"memlog\"|\"MEMLOG\"|\"mem_log\"|\"MEM_LOG\""),
        vec!["memlog", "mem_log"],
        "the shouted forms are the same two spellings"
    );
}

#[test]
fn a_pattern_with_no_word_at_all_yields_nothing() {
    assert!(lexer_spellings("{EAT_ABLE}").is_empty());
    assert!(lexer_spellings("").is_empty());
}

/// The reported class: a spelling the manual skips still hovers.
#[test]
fn an_undocumented_spelling_hovers_as_the_setting_it_is() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let documented = core
        .params
        .iter()
        .find(|p| p.name == "wdir")
        .expect("`wdir` is the documented spelling");
    let alias = core
        .params
        .iter()
        .find(|p| p.name == "workdir")
        .expect("`workdir` is accepted by the lexer and must be offered too");
    assert_eq!(
        alias.doc, documented.doc,
        "the same setting must not have two different answers"
    );
    assert!(
        alias.detail.contains("wdir"),
        "and it should say which spelling the manual uses: {:?}",
        alias.detail
    );
}

#[test]
fn every_alias_the_lexer_accepts_is_in_the_catalogue() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let names: Vec<&str> = core.params.iter().map(|p| p.name.as_str()).collect();
    for (documented, alias) in [
        ("wdir", "workdir"),
        ("tcp_threshold", "tcpthreshold"),
        ("exec_msg_threshold", "execmsgthreshold"),
        ("exec_dns_threshold", "execdnsthreshold"),
        ("mem_warming", "mem_warming_enabled"),
    ] {
        assert!(names.contains(&documented), "{documented} missing");
        assert!(
            names.contains(&alias),
            "`{alias}` is accepted and hovers nothing"
        );
    }
}

/// Documented under NO spelling, and still real.
#[test]
fn a_setting_the_manual_never_mentions_is_still_offered() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    for name in ["memdump", "memlog"] {
        let it = core
            .params
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("`{name} = 3` is accepted and offers nothing"));
        assert!(
            it.doc.to_lowercase().contains("no manual page describes"),
            "it must say the manual is silent rather than invent a description: \
             {:?}",
            it.doc
        );
    }
}

#[test]
fn a_script_call_the_manual_never_mentions_is_still_offered() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    for name in ["xdbg", "error"] {
        let it = core
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("`{name}(...)` is a call the grammar accepts"));
        assert!(!it.doc.trim().is_empty(), "{name} would hover blank");
    }
}

/// A documented entry must never be overwritten by a generated note.
#[test]
fn the_manual_wins_wherever_it_speaks() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let listen = core.params.iter().find(|p| p.name == "socket").unwrap();
    assert!(
        !listen
            .doc
            .to_lowercase()
            .contains("no manual page describes"),
        "`socket` is documented at length; a note must not have replaced it"
    );
    // POSITIVE CONTROL: the notes do exist somewhere, so this is not
    // passing because nothing was generated at all.
    assert!(
        core.params
            .iter()
            .any(|p| p.doc.to_lowercase().contains("no manual page describes")),
        "no note was generated anywhere — the reconciliation did not run"
    );
}

/// The real tree decides the numbers, not this test.
#[test]
fn the_reconciliation_covers_the_real_tree() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let core = opensips_lsp::catalog::harvest_core(&tree);
    let names: Vec<&str> = core.params.iter().map(|p| p.name.as_str()).collect();

    // POSITIVE CONTROL: a harvest that read nothing must say so here.
    assert!(
        names.len() > 90,
        "only {} parameters harvested",
        names.len()
    );
    for want in ["workdir", "memdump", "memlog"] {
        assert!(names.contains(&want), "{want} missing from a real harvest");
    }
    assert!(
        core.functions.iter().any(|f| f.name == "xdbg"),
        "xdbg missing from a real harvest"
    );
}

/// A name written like a call is not always a call.
///
/// `modparam("mod","p",v)` is a directive and `not(...)` is an
/// operator; neither is something a route can call. What separates a
/// script function in this grammar is that its production builds an
/// action, which is precisely the thing a route body executes.
/// Reading every `TOKEN LPAREN` as a function offered both of those
/// as core functions "the manual does not describe".
#[test]
fn a_directive_or_an_operator_is_not_a_core_function() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let names: Vec<&str> = core.functions.iter().map(|f| f.name.as_str()).collect();
    for not_a_function in ["modparam", "not"] {
        assert!(
            !names.contains(&not_a_function),
            "`{not_a_function}` is written like a call but a route cannot call \
             it: {} functions",
            names.len()
        );
    }
    // POSITIVE CONTROL: the ones that DO build an action are there
    for real in ["xdbg", "error"] {
        assert!(
            names.contains(&real),
            "{real} missing: {} functions",
            names.len()
        );
    }
}

/// The same, against a HARVEST rather than the vendored catalogue.
///
/// A test that reads `builtin_core()` guards the shipped artefact: it
/// notices a bad regeneration, and it cannot notice the rule itself
/// changing until someone regenerates. The rule needs its own test.
#[test]
fn a_harvest_excludes_directives_and_operators() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let core = opensips_lsp::catalog::harvest_core(&tree);
    let names: Vec<&str> = core.functions.iter().map(|f| f.name.as_str()).collect();

    // POSITIVE CONTROL: the harvest read functions at all.
    assert!(names.len() > 40, "only {} functions harvested", names.len());
    for not_a_function in ["modparam", "not"] {
        assert!(
            !names.contains(&not_a_function),
            "`{not_a_function}` is written like a call and builds no action, so a \
             route cannot call it: {} functions",
            names.len()
        );
    }
    for real in ["xdbg", "error"] {
        assert!(
            names.contains(&real),
            "{real} builds an action and is missing"
        );
    }
}

/// One name, one answer.
///
/// The reconciliation adds spellings the manual skipped. Adding one
/// that is ALREADY in the catalogue gives a name two entries, and
/// every lookup is a `find` — so the second is unreachable, sits in
/// completion twice, and the two can disagree.
#[test]
fn the_reconciliation_leaves_no_duplicate_names() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let core = opensips_lsp::catalog::harvest_core(&tree);
    for (what, items) in [("parameters", &core.params), ("functions", &core.functions)] {
        let mut seen: Vec<&str> = Vec::new();
        let mut dupes: Vec<&str> = Vec::new();
        for i in items {
            if seen.contains(&i.name.as_str()) {
                dupes.push(&i.name);
            } else {
                seen.push(&i.name);
            }
        }
        assert!(dupes.is_empty(), "duplicate {what}: {dupes:?}");
        // POSITIVE CONTROL: the list was non-trivial
        assert!(seen.len() > 40, "only {} {what}", seen.len());
    }
}
