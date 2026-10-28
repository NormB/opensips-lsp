//! `catalog::parse_readme_md`: module parameters and functions read from a
//! module's `README.md`, as the 4.x documentation writes them.

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

/// A 4.x README shaped the way OpenSIPS actually writes them.
///
/// The fixture above was hand-written and had no fenced examples, no
/// sub-headings and no cramped type annotation, so it agreed with the
/// parser rather than with the documentation the parser has to read.
/// Every real module README has all three.
const REAL_SHAPE: &str = r#"# rtpengine Module

## Admin Guide

### Exported Parameters

#### rtpengine_sock (string)

Definition of socket(s) used to connect to a proxy.

```opensips title="Set rtpengine_sock parameter"
...
# single rtproxy
modparam("rtpengine", "rtpengine_sock", "udp:localhost:12221")
#### not_a_param (integer)
...
```

#### connect_timeout (integer)

How long to wait for the connection.

##### Authentication

A sub-heading inside one parameter's prose.

#### db_url(str)

The type written without a space before it.

### Exported Functions

#### rtpengine_offer([flags])

Offer.
"#;

#[test]
fn a_fenced_example_does_not_end_the_parameter_section() {
    let m = parse_readme_md("rtpengine", REAL_SHAPE).expect("fixture parses");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"connect_timeout"),
        "a `#` comment inside a fenced example ended the section: {names:?}"
    );
}

#[test]
fn a_heading_inside_a_fence_is_not_an_item() {
    let m = parse_readme_md("rtpengine", REAL_SHAPE).unwrap();
    assert!(
        !m.params.iter().any(|p| p.name == "not_a_param"),
        "an example showing markdown was harvested as documentation"
    );
}

#[test]
fn a_sub_heading_in_a_parameters_prose_does_not_end_the_section() {
    let m = parse_readme_md("rtpengine", REAL_SHAPE).unwrap();
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"db_url"), "{names:?}");
    assert!(
        !names.iter().any(|n| n.contains("Authentication")),
        "the sub-heading became a parameter: {names:?}"
    );
}

#[test]
fn a_type_written_without_a_space_is_not_part_of_the_name() {
    let m = parse_readme_md("rtpengine", REAL_SHAPE).unwrap();
    let p = m
        .params
        .iter()
        .find(|p| p.name == "db_url")
        .expect("db_url is the name a modparam would write");
    assert_eq!(p.detail, "str");
}

#[test]
fn functions_after_an_example_are_still_harvested() {
    let m = parse_readme_md("rtpengine", REAL_SHAPE).unwrap();
    let names: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["rtpengine_offer"]);
}

/// One heading, several parameters.
///
/// `osp` documents `private_key, local_certificate, ca_certificates`
/// under a single heading, and `tls_mgm` does the same with
/// `server_domain, client_domain (string)`.  Read as one name the
/// entry matched no `modparam` at all, so every configuration setting
/// any of them was warned about a parameter that exists.  Only a
/// plain identifier counts as one of the names: `sp1_uri, sp2_uri,
/// ..., sp16_uri` elides the middle, and the elision is not a
/// parameter.
#[test]
fn a_heading_listing_several_parameters_documents_each_of_them() {
    let md = "# m\n\n### Exported Parameters\n\n#### private_key, local_certificate, ca_certificates\n\nThe TLS material.\n\n#### server_domain, client_domain (string)\n\nA domain.\n\n#### sp1_uri, sp2_uri, ..., sp16_uri\n\nService points.\n";
    let m = parse_readme_md("osp", md).expect("parses");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "private_key",
            "local_certificate",
            "ca_certificates",
            "server_domain",
            "client_domain",
            "sp1_uri",
            "sp2_uri",
            "sp16_uri",
        ],
        "{names:?}"
    );
    // the type and the prose belong to every name the heading listed
    assert_eq!(m.params[3].detail, "string");
    assert_eq!(m.params[4].detail, "string");
    assert_eq!(m.params[0].doc, m.params[2].doc);
    assert!(!m.params[0].doc.is_empty());
}

/// A comma inside a function signature is not a list of functions.
#[test]
fn a_function_signature_with_commas_is_still_one_function() {
    let md = "# m\n\n### Exported Functions\n\n#### jwt_db_authorize(jwt_token, out_decoded_token, out_sip_username)\n\nAuthorize.\n";
    let m = parse_readme_md("auth_jwt", md).unwrap();
    let names: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["jwt_db_authorize"], "{names:?}");
}

/// A parameter name that is not an identifier stays whole.
///
/// `b2b_sca` documents `app[index]_call_state_column`, a template
/// rather than a name; splitting it would invent parameters upstream
/// never named.
#[test]
fn a_templated_parameter_name_is_left_alone() {
    let md = "# m\n\n### Exported Parameters\n\n#### app[index]_call_state_column(string)\n\nA column.\n";
    let m = parse_readme_md("b2b_sca", md).unwrap();
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["app[index]_call_state_column"], "{names:?}");
}

/// A section heading written one level deeper is still a section.
///
/// `cachedb_couchbase`, `cachedb_memcached` and `cachedb_sql` write
/// `#### Exported Functions`, nested inside the parameters chapter
/// rather than beside it.  Read as an item of the open section it
/// became a PARAMETER named `Exported Functions` — an entry no
/// `modparam` could write, sitting in the catalogue of three modules.
#[test]
fn a_section_heading_at_item_depth_is_not_an_item() {
    let md = "# m\n\n### Exported Parameters\n\n#### cachedb_url (string)\n\nThe URL.\n\n#### Exported Functions\n\nThe module does not export functions.\n";
    let m = parse_readme_md("cachedb_sql", md).expect("parses");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["cachedb_url"], "{names:?}");
    assert!(m.functions.is_empty(), "{:?}", m.functions);
}

/// …and the items under such a heading belong to it.
#[test]
fn items_under_a_deeper_section_heading_are_that_sections() {
    let md = "# m\n\n### Exported Parameters\n\n#### cachedb_url (string)\n\nThe URL.\n\n#### Exported Functions\n\n#### cache_query(key)\n\nQuery it.\n";
    let m = parse_readme_md("cachedb_sql", md).unwrap();
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    let funcs: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(params, vec!["cachedb_url"], "{params:?}");
    assert_eq!(funcs, vec!["cache_query"], "{funcs:?}");
}

/// One heading that documents two functions.
///
/// `uac` writes `#### uac_replace_from([display],uri)
/// uac_replace_to([display],uri)` and `rtpproxy` separates its pair
/// with a comma. Only the first name was harvested, so
/// `uac_replace_to`, `uac_restore_to`, `rtpproxy_stream2uas`,
/// `rtpproxy_stop_stream2uas` and `t_write_unix` had no completion,
/// hover or signature help, although the C sources export each one.
#[test]
fn a_heading_naming_two_functions_documents_both() {
    let md = "# m\n\n### Exported Functions\n\n\
              #### uac_replace_from([display],uri) uac_replace_to([display],uri)\n\n\
              Replace the display name or the URI.\n\n\
              #### rtpproxy_stream2uac(prompt_name, count[, [set_id][, sock_var]]), rtpproxy_stream2uas(prompt_name, count[, [set_id][, sock_var]])\n\n\
              Stream a prompt.\n";
    let m = parse_readme_md("m", md).unwrap();
    let got: Vec<(&str, &str, &str)> = m
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f.detail.as_str(), f.doc.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "uac_replace_from",
                "uac_replace_from([display],uri)",
                "Replace the display name or the URI."
            ),
            (
                "uac_replace_to",
                "uac_replace_to([display],uri)",
                "Replace the display name or the URI."
            ),
            (
                "rtpproxy_stream2uac",
                "rtpproxy_stream2uac(prompt_name, count[, [set_id][, sock_var]])",
                "Stream a prompt."
            ),
            (
                "rtpproxy_stream2uas",
                "rtpproxy_stream2uas(prompt_name, count[, [set_id][, sock_var]])",
                "Stream a prompt."
            ),
        ]
    );
}

/// The negative controls: a pseudo-variable in the argument list is
/// not a second function, and two signatures of ONE function are one
/// entry, its heading unchanged.
#[test]
fn argument_parentheses_and_overloads_are_one_function() {
    let md = "# m\n\n### Exported Functions\n\n\
              #### json_link($json(dest_id), $json(source_id))\n\nLink.\n\n\
              #### record_route() and record_route(string)\n\nAdd a Record-Route.\n";
    let m = parse_readme_md("m", md).unwrap();
    let got: Vec<(&str, &str)> = m
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f.detail.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("json_link", "json_link($json(dest_id), $json(source_id))"),
            ("record_route", "record_route() and record_route(string)"),
        ]
    );
}

/// A parameter heading that names nothing gives no parameter: an empty
/// name would be offered as a completion and checked against every
/// `modparam`. Found by the `readme` fuzz target.
#[test]
fn a_parameter_heading_with_no_name_gives_no_parameter() {
    for heading in ["#### ", "####  ", "#### (integer)", "#### (str)"] {
        let md =
            format!("### Exported Parameters\n\n{heading}\n\nText.\n\n#### real (int)\n\nMore.\n");
        let m = parse_readme_md("x", &md).expect("parses");
        let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["real"], "{heading:?}");
    }
}
