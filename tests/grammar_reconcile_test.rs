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

/// Owed, first. A spelling may contain DIGITS.
///
/// The word filter accepted lower-case letters and underscores only,
/// so `tcp_source_ipv4` and `tcp_source_ipv6` were dropped without a
/// sound — settings the grammar accepts, silently absent from the
/// very reconciliation whose whole job is to find them. A filter that
/// discards its input is the shape of gap this work exists to close.
#[test]
fn a_spelling_may_contain_digits() {
    assert_eq!(
        lexer_spellings("\"tcp_source_ipv4\""),
        vec!["tcp_source_ipv4"],
        "a digit in the middle of a name is part of the name"
    );
    assert_eq!(
        lexer_spellings("\"disable_503_translation\""),
        vec!["disable_503_translation"]
    );
}

/// Owed, second. A bare number is not a spelling.
///
/// Widening the filter to allow digits must not widen it to allow a
/// token that is only digits: a port or a size in a lexer pattern is
/// a value, not a name anyone writes on the left of an `=`.
#[test]
fn a_bare_number_is_not_a_spelling() {
    assert!(
        lexer_spellings("\"5060\"").is_empty(),
        "a number is a value, not a setting name"
    );
    assert!(lexer_spellings("\"4xx\"").is_empty(), "nor is `4xx`");
    // POSITIVE CONTROL: the same shape starting with a letter is one
    assert_eq!(lexer_spellings("\"x4\""), vec!["x4"]);
}

/// Owed #1: nothing the grammar uses is dropped without a trace.
///
/// Both filters that failed accepted letters and underscores only,
/// and a name they rejected simply vanished — no error, no count, no
/// difference in the catalogue anyone would notice. The pair of tests
/// written when that was found pinned DIGITS. This pins the class:
/// every token the grammar actually assigns to must yield at least
/// one spelling, whatever characters upstream chooses next.
#[test]
fn no_assignable_token_is_silently_dropped_by_the_spelling_reader() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let lex = std::fs::read_to_string(tree.join("cfg.lex")).expect("cfg.lex");
    let y = std::fs::read_to_string(tree.join("cfg.y")).expect("cfg.y");

    let assignable: std::collections::BTreeSet<&str> = y
        .match_indices(" EQUAL")
        .filter_map(|(i, _)| y[..i].rsplit(|c: char| c.is_whitespace()).next())
        .filter(|t| !t.is_empty() && t.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        .collect();
    // POSITIVE CONTROL: the grammar was read.
    assert!(
        assignable.len() > 50,
        "only {} assignable tokens",
        assignable.len()
    );

    let mut dropped: Vec<&str> = Vec::new();
    let mut seen = 0usize;
    for line in lex.split("\n%%").next().unwrap_or(&lex).lines() {
        let mut it = line.splitn(2, [' ', '\t']);
        let (Some(tok), Some(pat)) = (it.next(), it.next()) else {
            continue;
        };
        if !assignable.contains(tok) {
            continue;
        }
        // punctuation is not a name: `RBRACK` is `]`, and it reaches
        // this list because `... RBRACK EQUAL ...` appears in the
        // grammar. A token whose pattern holds no lower-case letter
        // has no spelling to find, and demanding one would make this
        // gate unfixable.
        if !pat.chars().any(|c| c.is_ascii_lowercase()) {
            continue;
        }
        seen += 1;
        if opensips_lsp::catalog::lexer_spellings(pat).is_empty() {
            dropped.push(tok);
        }
    }
    assert!(
        seen > 40,
        "only {seen} assignable tokens matched a lexer line"
    );
    assert!(
        dropped.is_empty(),
        "the lexer defines these and the reader yields no spelling for any of \
         them, so they are absent from every hover and every completion with \
         nothing anywhere saying so: {dropped:?}"
    );
}

/// Owed #2: a token NAME may carry a digit.
///
/// The spelling filter was fixed first and the catalogue did not
/// move, because the token-name filter one level above was throwing
/// the line away before its spelling was ever read. Two filters, the
/// same omission, and fixing the visible one changed nothing.
#[test]
fn a_token_name_carrying_a_digit_is_read() {
    const LEX: &str = "TCP_SOURCE_IPV4\t\t\"tcp_source_ipv4\"\n%%\n";
    const Y: &str = "socket_def_param: TCP_SOURCE_IPV4 { x; } ;\nsocket_def_params: x;\n";
    assert_eq!(
        opensips_lsp::catalog::parse_socket_modifiers_c(Y, LEX),
        vec!["tcp_source_ipv4"],
        "a digit in the TOKEN name must not discard the line"
    );
}

/// Owed #3: no catalogue name can contain a separator.
///
/// The sibling server's cookbook documents families under one
/// heading — `### tcp_source_ipv4, tcp_source_ipv6` — and taking the
/// first word produced a parameter literally named
/// `tcp_source_ipv4,`. This manual uses no such heading today: this
/// is the guard for the day it does, because a name with a comma or
/// a space in it can never be hovered and nothing else would notice.
#[test]
fn no_catalogue_name_contains_a_separator() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let mut bad: Vec<&str> = Vec::new();
    let mut n = 0usize;
    for list in [
        &core.params,
        &core.functions,
        &core.statements,
        &core.routes,
    ] {
        for i in list {
            n += 1;
            if i.name.contains(',') || i.name.contains(char::is_whitespace) {
                bad.push(&i.name);
            }
        }
    }
    // POSITIVE CONTROL: the catalogue was read.
    assert!(n > 150, "only {n} entries scanned");
    assert!(
        bad.is_empty(),
        "a name carrying a separator is one no reader can ever hover, and it \
         means a heading named several things: {bad:?}"
    );
}

/// Owed #4: and the same for a fresh harvest, not just the artefact.
#[test]
fn no_harvested_name_contains_a_separator() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let core = opensips_lsp::catalog::harvest_core(&tree);
    let mut bad: Vec<&str> = Vec::new();
    let mut n = 0usize;
    for list in [
        &core.params,
        &core.functions,
        &core.statements,
        &core.routes,
    ] {
        for i in list {
            n += 1;
            if i.name.contains(',') || i.name.contains(char::is_whitespace) {
                bad.push(&i.name);
            }
        }
    }
    assert!(n > 150, "only {n} entries harvested");
    assert!(bad.is_empty(), "{bad:?}");
}

// ---------------------------------------------------------------
// Owed for the four failures above. Two defects sat under them: one
// spelling reader written TWICE, whose copies drifted so the same
// lexer line read differently depending on which asked; and filters
// that discarded valid input without a trace.
//
// The dangerous direction here is not a missed warning — nothing
// validates global names, so a generated entry cannot silence a
// check. It is INVENTION: a hover telling a reader that a setting
// exists when the grammar has never heard of it. A name that is
// merely missing is a gap; a name that is fabricated is a lie the
// reader has no way to test.
// ---------------------------------------------------------------

/// Owed 1/8: one reader, checked against the other on every line.
///
/// `parse_socket_modifiers_c` carried its own copy of the spelling
/// logic. The shared one was widened for digits and the copy was not,
/// so the same lexer line yielded a spelling to one caller and
/// nothing to the other. This holds them together over the whole real
/// file rather than over a fixture that would agree by construction.
#[test]
fn the_two_spelling_readers_agree_on_every_line_of_the_real_lexer() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let lex = std::fs::read_to_string(tree.join("cfg.lex")).expect("cfg.lex");
    let mut checked = 0usize;
    for line in lex.split("\n%%").next().unwrap_or(&lex).lines() {
        let mut it = line.splitn(2, [' ', '\t']);
        let (Some(tok), Some(pat)) = (it.next(), it.next()) else {
            continue;
        };
        if tok.is_empty() || !tok.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let shared = opensips_lsp::catalog::lexer_spellings(pat);
        // the socket path resolves one token to one spelling
        let y = format!("socket_def_param: {tok} {{ x; }} ;\nsocket_def_params: x;\n");
        let via_socket = opensips_lsp::catalog::parse_socket_modifiers_c(&y, &lex);
        checked += 1;
        assert_eq!(
            via_socket,
            shared.iter().take(1).cloned().collect::<Vec<_>>(),
            "the two readers disagree about `{tok}` ({pat:?})"
        );
    }
    // POSITIVE CONTROL: the file was read.
    assert!(checked > 200, "only {checked} token lines compared");
}

/// Owed 2/8: a hyphenated name reaches the socket path too.
///
/// `mem-group` is a real core parameter of this server. A reader that
/// stops at `_` drops it, and the copy that drifted stopped there
/// after the shared one had been fixed.
#[test]
fn a_hyphenated_spelling_reaches_every_reader() {
    const LEX: &str = "MEMGROUP\tmem-group\n%%\n";
    assert_eq!(
        opensips_lsp::catalog::lexer_spellings("mem-group"),
        vec!["mem-group"]
    );
    const Y: &str = "socket_def_param: MEMGROUP { x; } ;\nsocket_def_params: x;\n";
    assert_eq!(
        opensips_lsp::catalog::parse_socket_modifiers_c(Y, LEX),
        vec!["mem-group"],
        "and the socket reader must not stop at the underscore either"
    );
}

/// Owed 3/8: every spelling is something a reader could actually type.
///
/// Widening a filter twice invites the opposite failure — accepting
/// fragments of a regex as if they were names. Run over the real
/// lexer, every spelling this reader produces must look like a
/// configuration word.
#[test]
fn every_spelling_from_the_real_lexer_is_writable() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let lex = std::fs::read_to_string(tree.join("cfg.lex")).expect("cfg.lex");
    let mut bad: Vec<String> = Vec::new();
    let mut n = 0usize;
    for line in lex.split("\n%%").next().unwrap_or(&lex).lines() {
        let Some((_, pat)) = line.split_once([' ', '\t']) else {
            continue;
        };
        for w in opensips_lsp::catalog::lexer_spellings(pat) {
            n += 1;
            let writable = w.starts_with(|c: char| c.is_ascii_lowercase())
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
            if !writable {
                bad.push(format!("{w:?} from {pat:?}"));
            }
        }
    }
    assert!(n > 150, "only {n} spellings produced");
    assert!(
        bad.is_empty(),
        "these are not names anyone can write — a regex fragment read as a \
         setting is worse than a missing one, because a hover would claim it \
         exists: {bad:?}"
    );
}

/// Owed 4/8: the reconciliation never invents a name.
///
/// This is the dangerous direction. Nothing validates global names,
/// so a generated entry cannot silence a warning — it can only tell a
/// reader that a setting exists. Every name the reconciliation adds
/// must appear VERBATIM in the lexer that is supposed to accept it.
#[test]
fn every_name_the_reconciliation_adds_appears_in_the_lexer() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let lex = std::fs::read_to_string(tree.join("cfg.lex")).expect("cfg.lex");
    let manual_only = {
        // what the manual alone yields, before any grammar reading
        let md =
            |f: &str| std::fs::read_to_string(tree.join("docs/manual").join(f)).unwrap_or_default();
        let mut v = opensips_lsp::catalog::parse_core_params_md(&md("Script-CoreParameters.md"))
            .unwrap_or_default();
        v.extend(
            opensips_lsp::catalog::parse_core_functions_md(&md("Script-CoreFunctions.md"))
                .unwrap_or_default(),
        );
        v.into_iter().map(|i| i.name).collect::<Vec<_>>()
    };
    let core = opensips_lsp::catalog::harvest_core(&tree);

    let mut invented: Vec<&str> = Vec::new();
    let mut generated = 0usize;
    for i in core.params.iter().chain(core.functions.iter()) {
        if manual_only.contains(&i.name) {
            continue;
        }
        generated += 1;
        if !lex.contains(&i.name) {
            invented.push(&i.name);
        }
    }
    // POSITIVE CONTROL: the reconciliation added something, or this
    // gate is comparing an empty set against the lexer.
    assert!(generated > 5, "only {generated} generated entries");
    assert!(
        invented.is_empty(),
        "the catalogue offers these and the lexer has never heard of them, so a \
         hover would claim a setting exists that no configuration can use: \
         {invented:?}"
    );
}

/// Owed for the `SSLv23` failure: a token NAME may be mixed case.
///
/// Every narrowing of this filter has cost the same way. First it
/// rejected digits, so `TCP_SOURCE_IPV4` vanished. Then it rejected
/// lower-case letters, so `SSLv23` — a real token of this grammar —
/// vanished too. Both in silence, which is what makes the class
/// worth a test rather than a fix.
#[test]
fn a_mixed_case_token_name_is_read() {
    const LEX: &str = "SSLv23\t\t\"sslv23\"|\"SSLv23\"\n%%\n";
    assert_eq!(
        opensips_lsp::catalog::lexer_spellings("\"sslv23\"|\"SSLv23\""),
        vec!["sslv23"]
    );
    assert_eq!(
        opensips_lsp::catalog::parse_socket_modifiers_c(
            "socket_def_param: SSLv23 { x; } ;\nsocket_def_params: x;\n",
            LEX,
        ),
        vec!["sslv23"],
        "a lower-case letter in the TOKEN name must not discard the line"
    );
}

/// And the filter still refuses what is not a token at all.
#[test]
fn punctuation_and_lower_case_starts_are_not_token_names() {
    // a token name starts upper-case; these are grammar punctuation
    // or non-terminals, and accepting them would sweep the entire
    // grammar into the catalogue
    for not_a_token in ["socket_def", "listen_id", "1ABC", "_X"] {
        assert!(
            opensips_lsp::catalog::parse_socket_modifiers_c(
                &format!("socket_def_param: {not_a_token} {{ x; }} ;\nsocket_def_params: x;\n"),
                &format!("{not_a_token}\tvalue\n%%\n"),
            )
            .is_empty(),
            "`{not_a_token}` is not a token name"
        );
    }
}
