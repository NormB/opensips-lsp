//! `socket = udp:1.2.3.4:5060 use_workers 4` — the tail hovers too.
//!
//! GIVEN a `socket` line may carry twelve optional modifiers,
//! WHEN a reader hovers `use_workers` or `reuse_port`,
//! THEN they get what that modifier does.
//!
//! They got nothing. `socket` hovered, and every word after it was
//! invisible — which is most of what a real `socket` line is made of.
//!
//! Membership comes from the grammar's `socket_def_param` production
//! and the spellings from the lexer, the same way module parameters
//! come from `param_export_t`: the manual's bullet list supplies the
//! TEXT, but a modifier the grammar accepts is a modifier whether the
//! manual mentions it or not.
//!
//! Only `socket =` takes them — nothing else in the grammar uses that
//! production — so `as`, `tag` and `frag` hover there and nowhere
//! else. They are ordinary words; answering for them anywhere in a
//! configuration would be worse than answering nowhere.

mod common;

use opensips_lsp::catalog::{parse_socket_modifiers_c, parse_socket_modifiers_md};

const LEX: &str = r#"
USE_WORKERS	{EAT_ABLE}("use_workers"|"USE_WORKERS"){EAT_ABLE}
REUSE_PORT	("reuse_port"|"REUSE_PORT")
TAG		 tag
%%
"#;

const Y: &str = r#"
socket_def_param: REUSE_PORT { IFOR();
					p_tmp.flags |= SI_REUSEPORT;
					}
				| USE_WORKERS NUMBER { IFOR();
					p_tmp.workers=$2;
					}
				| TAG ID { IFOR();
					p_tmp.tag = $2;
					}
				;

socket_def_params:	socket_def_param
				 |	socket_def_param socket_def_params
				 ;
"#;

const PAGE: &str = r#"### socket

Sets the sockets to listen on.

The `socket` definition may accept several optional parameters:
* `use_workers n`: set a different number of workers for this socket only.
* `reuse_port`: allow outgoing connections to reuse the listening port.

Example of usage:
```opensips

    socket = udp:1.2.3.4:5060

```

### udp_workers

Global worker count.
"#;

#[test]
fn membership_comes_from_the_grammar() {
    let got = parse_socket_modifiers_c(Y, LEX);
    assert_eq!(got, vec!["reuse_port", "use_workers", "tag"]);
}

#[test]
fn a_production_that_is_not_the_socket_one_is_not_read() {
    let other = "other_param: REUSE_PORT { x; } ;\n";
    assert!(
        parse_socket_modifiers_c(other, LEX).is_empty(),
        "only the production `socket =` uses is the socket modifier list"
    );
}

#[test]
fn the_manual_supplies_the_text() {
    let got = parse_socket_modifiers_md(PAGE);
    let uw = got
        .iter()
        .find(|i| i.name == "use_workers")
        .expect("use_workers");
    assert!(
        uw.doc.contains("different number of workers"),
        "{:?}",
        uw.doc
    );
    assert!(
        uw.detail.contains("use_workers n"),
        "the written form is what a reader needs to type: {:?}",
        uw.detail
    );
}

#[test]
fn bullets_outside_the_socket_section_are_not_modifiers() {
    let got = parse_socket_modifiers_md(
        "### other\n\nThings:\n* `not_a_modifier`: no.\n\n### socket\n\nS.\n\nopts:\n* `frag`: yes.\n",
    );
    let names: Vec<&str> = got.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["frag"], "{names:?}");
}

/// The real tree: the grammar and the manual must agree about the set.
#[test]
fn the_real_grammar_and_manual_describe_the_same_modifiers() {
    let tree = std::path::PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let y = std::fs::read_to_string(tree.join("cfg.y")).expect("cfg.y");
    let lex = std::fs::read_to_string(tree.join("cfg.lex")).expect("cfg.lex");
    let page = std::fs::read_to_string(tree.join("docs/manual/Script-CoreParameters.md"))
        .expect("the parameters page");

    let from_c = parse_socket_modifiers_c(&y, &lex);
    let documented: Vec<String> = parse_socket_modifiers_md(&page)
        .into_iter()
        .map(|i| i.name)
        .collect();

    // POSITIVE CONTROL: neither scan may be empty, or the comparison
    // below is two nothings agreeing.
    assert!(from_c.len() >= 10, "grammar gave {from_c:?}");
    assert!(documented.len() >= 10, "manual gave {documented:?}");

    let undocumented: Vec<&String> = from_c.iter().filter(|m| !documented.contains(m)).collect();
    assert!(
        undocumented.is_empty(),
        "the grammar accepts these and the manual describes none of them — they \
         are still modifiers, and must still be offered: {undocumented:?}"
    );
}

#[test]
fn the_built_in_catalogue_carries_them() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let names: Vec<&str> = core
        .socket_modifiers
        .iter()
        .map(|i| i.name.as_str())
        .collect();
    assert!(names.len() >= 10, "{names:?}");
    for m in ["use_workers", "reuse_port", "anycast", "tag", "frag", "as"] {
        assert!(names.contains(&m), "{m} missing: {names:?}");
        let it = core.socket_modifiers.iter().find(|i| i.name == m).unwrap();
        assert!(!it.doc.trim().is_empty(), "{m} would hover blank");
    }
}

#[test]
fn hovering_a_modifier_on_a_socket_line_answers() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let text = "socket=udp:10.0.0.1:5060 use_workers 4\nsocket=tcp:10.0.0.1:5060 reuse_port\n";
    for (w, line, col) in [("use_workers", 0u32, 25u32), ("reuse_port", 1, 25)] {
        let got = opensips_lsp::logic::hover_markdown_at(&[], core, text, w, line, col)
            .unwrap_or_else(|| panic!("{w} must hover on a socket line"));
        assert!(got.contains(w), "must name it: {got:?}");
        assert!(
            got.to_lowercase().contains("socket"),
            "and say where it belongs: {got:?}"
        );
    }
}

/// `as`, `tag` and `frag` are ordinary words.
#[test]
fn a_modifier_word_elsewhere_does_not_hover_as_one() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let text = "route {\n    $var(tag) = 1;\n}\n";
    let got = opensips_lsp::logic::hover_markdown_at(&[], core, text, "tag", 1, 9);
    assert!(
        got.as_deref()
            .is_none_or(|h| !h.contains("socket modifier")),
        "answering for every `tag` in a configuration is worse than answering \
         for none: {got:?}"
    );
}

#[test]
fn a_socket_line_offers_the_modifiers() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let offered = opensips_lsp::logic::completions_with_core(
        &[],
        core,
        "socket=udp:1.2.3.4:5060 \n",
        "socket=udp:1.2.3.4:5060 ",
    );
    let labels: Vec<&str> = offered.iter().map(|c| c.label.as_str()).collect();
    for m in ["use_workers", "reuse_port", "anycast"] {
        assert!(labels.contains(&m), "{m} not offered: {labels:?}");
    }
}

#[test]
fn an_ordinary_line_offers_no_modifiers() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let offered = opensips_lsp::logic::completions_with_core(&[], core, "route {\n}\n", "    ");
    let labels: Vec<&str> = offered.iter().map(|c| c.label.as_str()).collect();
    assert!(
        !labels.contains(&"reuse_port"),
        "a socket modifier is only ever on a socket line: {} labels",
        labels.len()
    );
}

/// Owed, first. A spelling written in PARTS.
///
/// `ALLOW_PROXY_PROTOCOL` is
/// `("allow"|"ALLOW")[-_]("proxy"|"PROXY")([-_]("protocol"|"PROTOCOL"))?`
/// — three groups, joined. Reading only the first gave `allow`,
/// which is not a modifier anyone can write and is not what the
/// manual describes, so the two sets disagreed and the real modifier
/// was missing from both hover and completion.
#[test]
fn a_spelling_written_in_parts_is_read_whole() {
    const MULTI: &str = concat!(
        "ALLOW_PROXY_PROTOCOL\t(\"allow\"|\"ALLOW\")[-_](\"proxy\"|\"PROXY\")",
        "([-_](\"protocol\"|\"PROTOCOL\"))?\n%%\n"
    );
    let y = "socket_def_param: ALLOW_PROXY_PROTOCOL { x; } ;\nsocket_def_params: x;\n";
    assert_eq!(
        parse_socket_modifiers_c(y, MULTI),
        vec!["allow_proxy_protocol"],
        "the optional trailing group is part of the canonical spelling"
    );
}

/// Owed, second. One group with several lower-case alternatives is a
/// choice, not a sequence: joining them gives a word nobody wrote.
#[test]
fn alternatives_within_one_group_are_a_choice_not_a_sequence() {
    const ALTS: &str = "SOME_TOK\t(\"first\"|\"second\"|\"third\")\n%%\n";
    let y = "socket_def_param: SOME_TOK { x; } ;\nsocket_def_params: x;\n";
    assert_eq!(
        parse_socket_modifiers_c(y, ALTS),
        vec!["first"],
        "not `first_second_third`"
    );
}
