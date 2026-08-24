use opensips_lsp::catalog::parse_readme_md;

const FIXTURE: &str = r#"# TM Module

## Admin Guide

### Overview

Stuff.

### Exported Parameters

#### fr_timeout (integer)


Timeout which is triggered if no final reply
arrives (in seconds).


*Default value is 30 seconds.*

#### auto_100trying (integer)

Flag doc with a path C:\x\y and trailing text.

### Exported Functions

#### t_relay([flags],[outbound_proxy])


Relay a message statefully to destination indicated in current URI.
Second sentence same paragraph.

Second paragraph excluded.

#### t_newtran()

Creates a new transaction.

### Exported MI Functions

#### t_uac_dlg

Not a script function; must not be collected.
"#;

#[test]
fn parses_readme_params_and_functions() {
    let m = parse_readme_md("tm", FIXTURE).expect("fixture parses");
    assert_eq!(m.name, "tm");
    assert_eq!(m.params.len(), 2);
    assert_eq!(m.params[0].name, "fr_timeout");
    assert_eq!(m.params[0].detail, "integer");
    assert_eq!(
        m.params[0].doc,
        "Timeout which is triggered if no final reply arrives (in seconds)."
    );
    assert!(m.params[1].doc.contains(r"C:\x\y"));

    assert_eq!(m.functions.len(), 2);
    assert_eq!(m.functions[0].name, "t_relay");
    assert_eq!(m.functions[0].detail, "t_relay([flags],[outbound_proxy])");
    assert!(m.functions[0].doc.starts_with("Relay a message statefully"));
    assert!(!m.functions[0].doc.contains("Second paragraph excluded"));
    assert_eq!(m.functions[1].name, "t_newtran");
}

#[test]
fn mi_functions_are_not_script_functions() {
    let m = parse_readme_md("tm", FIXTURE).unwrap();
    assert!(!m.functions.iter().any(|f| f.name == "t_uac_dlg"));
}

#[test]
fn empty_and_nul_are_errors() {
    assert!(parse_readme_md("m", "").is_err());
    assert!(parse_readme_md("m", "a\0b").is_err());
}

#[test]
fn readme_without_export_sections_is_ok_and_empty() {
    let m = parse_readme_md("m", "# Hi\n\nJust prose.\n").unwrap();
    assert!(m.params.is_empty() && m.functions.is_empty());
}
