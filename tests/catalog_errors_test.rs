//! The documentation parsers and the cache writer report failures as
//! typed errors (Rust API Guidelines C-GOOD-ERR): each says which input it
//! refused, implements `std::error::Error`, and is `Send + Sync`.

mod common;

use std::error::Error;
use std::io;

use opensips_lsp::catalog;
use opensips_lsp::catalog::ParseError;

fn is_error<E: Error + Send + Sync + 'static>() {}

#[test]
fn the_error_types_are_well_behaved() {
    is_error::<ParseError>();
    is_error::<io::Error>();
}

#[test]
fn empty_input_is_the_empty_error() {
    assert_eq!(catalog::parse_admin_xml("m", ""), Err(ParseError::Empty));
    assert_eq!(catalog::parse_readme_md("m", ""), Err(ParseError::Empty));
    assert_eq!(catalog::parse_core_functions_md(""), Err(ParseError::Empty));
    assert_eq!(catalog::parse_core_params_md(""), Err(ParseError::Empty));
}

#[test]
fn a_nul_byte_is_the_nul_error() {
    assert_eq!(
        catalog::parse_admin_xml("m", "<a>\0</a>"),
        Err(ParseError::Nul)
    );
    assert_eq!(catalog::parse_readme_md("m", "a\0b"), Err(ParseError::Nul));
    assert_eq!(catalog::parse_core_vars_md("a\0b"), Err(ParseError::Nul));
    assert_eq!(catalog::parse_core_routes_md("a\0b"), Err(ParseError::Nul));
}

#[test]
fn malformed_xml_carries_the_xml_error_as_its_source() {
    let err = catalog::parse_admin_xml("m", "<chapter><section id=")
        .expect_err("a truncated attribute is not XML");
    assert!(
        matches!(err, ParseError::Xml(_)),
        "a truncated attribute is an XML error, not {err:?}"
    );
    assert!(
        err.source().is_some(),
        "the XML error is kept as the source"
    );
    assert!(
        err.to_string().starts_with("not well-formed XML: "),
        "the message names the cause: {err}"
    );
}

#[test]
fn each_error_message_says_what_was_refused() {
    assert_eq!(ParseError::Empty.to_string(), "empty input");
    assert_eq!(ParseError::Nul.to_string(), "input contains NUL bytes");
}

#[test]
fn a_cache_that_cannot_be_written_is_an_io_error() {
    let scratch = common::Scratch::new("oslsp-cache-error");
    std::fs::create_dir_all(scratch.path()).expect("the temporary directory is writable");
    // a file where the cache directory should be: create_dir_all fails
    let blocked = scratch.path().join("not-a-dir");
    std::fs::write(&blocked, b"x").expect("the scratch directory is writable");
    let err: io::Error =
        catalog::save_cache(scratch.path(), &blocked, &[], &catalog::CoreDocs::default())
            .expect_err("a file is not a directory");
    assert_ne!(
        err.kind(),
        io::ErrorKind::Other,
        "the io error kind is kept: {err}"
    );
}
