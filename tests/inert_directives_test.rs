//! `#!` directives that do nothing in OpenSIPS.
//!
//! OpenSIPS's `cfg.lex` has `COM_LINE #` and the rule
//! `<INITIAL>{COM_LINE}.*{CR} { count(); }`, and contains no
//! preprocessor token at all — verified against the real 4.0.1 tree by
//! the gate below.  So a config carried over from Kamailio still
//! parses while every `#!ifdef` in it has quietly stopped guarding
//! anything, which is the sort of silence worth a warning.

mod common;
use opensips_lsp::logic;

fn messages(text: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("oslsp-inert-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("t.cfg");
    let out = logic::analyzer_diagnostics(&path, text, &|_| None)
        .into_iter()
        .map(|d| format!("{}:{}: {}", d.line, d.col_start, d.message))
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn a_kamailio_directive_is_reported_as_having_no_effect() {
    let got = messages("#!ifdef USE_TCP\nroute { exit; }\n#!endif\n");
    assert_eq!(got.len(), 2, "both directives should be flagged: {got:?}");
    assert!(got[0].contains("`#!ifdef` has no effect"), "{got:?}");
    assert!(
        got[0].contains("no preprocessor"),
        "the message must say why: {got:?}"
    );
    assert!(got[1].contains("`#!endif`"), "{got:?}");
}

#[test]
fn every_directive_kamailio_defines_is_recognised() {
    // the ones that bind a name and the ones that branch; all of them
    // are inert here, so all of them are worth flagging
    for kw in [
        "define",
        "def",
        "ifdef",
        "ifndef",
        "endif",
        "trydef",
        "trydefine",
        "redefine",
        "subst",
        "substdef",
        "defenv",
        "defexp",
    ] {
        let got = messages(&format!("#!{kw} X\nroute {{ exit; }}\n"));
        assert!(
            got.iter().any(|m| m.contains(&format!("`#!{kw}`"))),
            "#!{kw} was not flagged: {got:?}"
        );
    }
}

/// Precision matters more than recall here: a false warning on every
/// `#!` line would train people to ignore the real ones.
#[test]
fn ordinary_comments_and_shebangs_are_left_alone() {
    let quiet = [
        "# an ordinary comment\nroute { exit; }\n",
        "#!/usr/bin/env opensips\nroute { exit; }\n",
        "#! not a directive at all\nroute { exit; }\n",
        "#!\nroute { exit; }\n",
    ];
    for text in quiet {
        assert!(
            messages(text).is_empty(),
            "false positive on {text:?}: {:?}",
            messages(text)
        );
    }
}

/// Text inside a block comment is somebody deliberately commenting
/// code out, and the warning must not chase it.
#[test]
fn a_directive_inside_a_block_comment_is_not_flagged() {
    let text = "/* parked for now\n#!ifdef USE_TCP\n*/\nroute { exit; }\n";
    assert!(messages(text).is_empty(), "{:?}", messages(text));
}

/// The claim the warning rests on, checked against the real thing:
/// OpenSIPS's lexer must still have no preprocessor.  If a future
/// release grows one, this warning becomes wrong and must go.
#[test]
fn the_real_opensips_lexer_still_has_no_preprocessor() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let lex = std::fs::read_to_string(std::path::Path::new(&tree).join("cfg.lex"))
        .expect("cfg.lex in the pinned tree");
    assert!(
        lex.contains("COM_LINE"),
        "the lexer no longer defines a line comment; re-read it"
    );
    for token in ["PREP_START", "IFDEF", "TRYDEF", "SUBSTDEF"] {
        assert!(
            !lex.contains(token),
            "OpenSIPS's lexer now has {token}: it has grown a preprocessor, \
             so `#!` lines are no longer inert and this warning is wrong"
        );
    }
}
