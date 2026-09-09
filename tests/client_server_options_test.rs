//! The initialization options are one fact written in two languages.
//!
//! The server pulls its options out of `initialization_options` by
//! name; the client builds that object by name in TypeScript. Nothing
//! connects the two, and the client's own suites cannot: they observe
//! which SETTINGS it reads, not which KEYS it forwards, so a payload
//! key that is renamed, misspelled or never added leaves every test
//! green while the server falls back to a default forever. That is not
//! hypothetical — `versionInHints` and `opensipsVersion` were read by
//! the server and absent from the client's payload, so the two settings
//! the manifest published for them did nothing at all.

use std::collections::BTreeSet;

/// Every name the server looks up in its options object.
///
/// Whitespace is stripped before the search because rustfmt wraps the
/// long ones as `opts\n    .get("name")`. A scan for the literal
/// `opts.get("` sees eight of the twelve and, worse, still satisfies a
/// count-based vacuity guard while missing exactly the ones that were
/// broken.
fn options_the_server_reads(src: &str) -> BTreeSet<String> {
    let flat: String = src.chars().filter(|c| !c.is_whitespace()).collect();
    flat.split("opts.get(\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

/// The keys of one brace-delimited object literal in the client, found
/// by its opening line and read to the brace that closes it.
///
/// Three things this has to get right, each of which it got wrong
/// first. Parenthesis depth, because the payload entries are written
/// `name: cfg.get<T>('setting', default)` and the long ones wrap so
/// that a bare `true,` or `100,` sits on a line of its own. Line
/// comments, because prose containing a colon reads as a key. And
/// identifiers must not run together across a newline. A key whose
/// value is itself an object is a wrapper rather than an option: the
/// two clients disagree about whether to nest the
/// `didChangeConfiguration` payload under the namespace, and the
/// server accepts either shape.
fn object_keys(src: &str, opener: &str) -> BTreeSet<String> {
    let start = match src.find(opener) {
        Some(i) => i + opener.len(),
        None => return BTreeSet::new(),
    };
    let chars: Vec<char> = src[start..].chars().collect();
    let mut keys = BTreeSet::new();
    let (mut brace, mut paren) = (1usize, 0usize);
    let mut ident = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            ident.clear();
            continue;
        }
        match chars[i] {
            '{' => brace += 1,
            '}' => {
                brace -= 1;
                if brace == 0 {
                    break;
                }
            }
            '(' | '[' => paren += 1,
            ')' | ']' => paren = paren.saturating_sub(1),
            c @ (':' | ',') if paren == 0 => {
                let wraps_an_object = c == ':'
                    && chars[i + 1..]
                        .iter()
                        .find(|c| !c.is_whitespace())
                        .is_some_and(|c| *c == '{');
                if !wraps_an_object
                    && ident.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                {
                    keys.insert(ident.clone());
                }
            }
            c if c.is_ascii_alphanumeric() || c == '_' || c == '$' => {
                ident.push(c);
                i += 1;
                continue;
            }
            _ => {}
        }
        ident.clear();
        i += 1;
    }
    keys
}

#[test]
fn every_option_the_server_reads_is_one_the_client_sends() {
    let root = env!("CARGO_MANIFEST_DIR");
    let server = std::fs::read_to_string(format!("{root}/src/server.rs")).unwrap();
    let client = std::fs::read_to_string(format!("{root}/client/src/extension.ts")).unwrap();

    let wanted = options_the_server_reads(&server);
    let mut sent = object_keys(&client, "initializationOptions: {");
    sent.extend(object_keys(&client, "settings: {"));

    // POSITIVE CONTROLS: an empty side would make the comparison below
    // pass by vacuity, and both sides are found by text search.
    assert!(
        wanted.len() >= 8,
        "found only {} options read by the server; the scan is broken, not the code: {wanted:?}",
        wanted.len()
    );
    assert!(
        sent.len() >= 8,
        "found only {} keys in the client's payloads; the scan is broken, not the code: {sent:?}",
        sent.len()
    );

    let missing: Vec<&String> = wanted.difference(&sent).collect();
    assert!(
        missing.is_empty(),
        "the server reads options the client never sends, so they can only \
         ever hold their defaults: {missing:?}"
    );

    // The other direction, which is what makes the pair mutually
    // constraining: a key that is misspelled on the client side is both
    // an option the server never receives AND a key nothing reads, so
    // neither half of this test can be fooled by a scan that quietly
    // stops finding names on the other side.
    let ignored: Vec<&String> = sent.difference(&wanted).collect();
    assert!(
        ignored.is_empty(),
        "the client sends options the server never reads, so they are \
         carried across the wire and dropped: {ignored:?}"
    );
}
