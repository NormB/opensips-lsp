//! Call-site scanning: the positions inlay hints attach to.
//!
//! A hint sits at the START of an argument, so what matters is that
//! every top-level argument is found at its first real character —
//! through nesting, through quotes, and never inside a string or a
//! comment.

mod common;
use opensips_lsp::analyze::calls;

fn shape(text: &str) -> Vec<(String, Vec<(u32, u32)>)> {
    calls(text).into_iter().map(|c| (c.name, c.args)).collect()
}

#[test]
fn arguments_are_found_at_their_first_real_character() {
    let text = "route {\n    t_relay(\"udp\", 1);\n}\n";
    assert_eq!(
        shape(text),
        vec![("t_relay".to_string(), vec![(1, 12), (1, 19)])]
    );
}

#[test]
fn a_call_with_no_arguments_has_none() {
    assert_eq!(
        shape("route {\n    t_newtran();\n}\n"),
        vec![("t_newtran".to_string(), vec![])]
    );
    // whitespace-only is still no argument
    assert_eq!(
        shape("route {\n    t_newtran(  );\n}\n"),
        vec![("t_newtran".to_string(), vec![])]
    );
}

#[test]
fn commas_inside_strings_and_nesting_do_not_split_arguments() {
    let text = "route {\n    f(\"a,b\", g(1, 2), 3);\n}\n";
    let got = shape(text);
    let outer = got.iter().find(|(n, _)| n == "f").expect("outer call");
    assert_eq!(
        outer.1.len(),
        3,
        "a comma in a string or nested call is not a separator: {got:?}"
    );
    // the nested call is a call in its own right
    assert!(got.iter().any(|(n, a)| n == "g" && a.len() == 2), "{got:?}");
}

#[test]
fn calls_in_strings_and_comments_are_not_calls() {
    let text = "route {\n    # t_relay(1);\n    $var(x) = \"t_relay(2)\";\n}\n";
    assert!(
        !shape(text).iter().any(|(n, _)| n == "t_relay"),
        "{:?}",
        shape(text)
    );
}

#[test]
fn a_multi_line_call_keeps_each_argument_on_its_own_line() {
    let text = "route {\n    f(\n        1,\n        2\n    );\n}\n";
    assert_eq!(shape(text), vec![("f".to_string(), vec![(2, 8), (3, 8)])]);
}

#[test]
fn whitespace_between_the_name_and_the_paren_is_allowed() {
    assert_eq!(
        shape("route {\n    f (1);\n}\n"),
        vec![("f".to_string(), vec![(1, 7)])]
    );
}

#[test]
fn adversarial_input_does_not_panic() {
    for s in [
        "",
        "f(",
        "f()",
        "f(\"unterminated",
        "f(((((",
        "f(\u{1F600})",
        "route {\n f(1,\n",
    ] {
        let _ = calls(s);
    }
}
