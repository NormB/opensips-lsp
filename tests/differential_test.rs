//! Differential gates against the REAL parser (the ground truth):
//!
//!   - the analyzer must be SILENT on every corpus config the binary
//!     accepts — any false positive on real-world configs is a CI
//!     failure, not an audit finding;
//!   - a rename applied through the real logic path must produce a
//!     config the binary still accepts.
//!
//! Both are env-gated: OPENSIPS_LSP_TEST_TREE / OPENSIPS_LSP_TEST_BIN.

mod common;

use opensips_lsp::logic;

/// Same enumeration as the corpus sweep: shipped example configs.
fn corpus(tree: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for dir in [tree.join("etc"), tree.join("examples")] {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "cfg") {
                out.push(p);
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(tree.join("modules")) {
        for e in rd.flatten() {
            for sub in ["examples", "etc"] {
                let d = e.path().join(sub);
                let Ok(rd2) = std::fs::read_dir(&d) else {
                    continue;
                };
                for f in rd2.flatten() {
                    let p = f.path();
                    if p.extension().is_some_and(|x| x == "cfg") {
                        out.push(p);
                    }
                }
            }
        }
    }
    out.sort();
    out
}

fn disk_loader(p: &std::path::Path) -> Option<String> {
    let md = std::fs::metadata(p).ok()?;
    if !md.is_file() || md.len() > 1_048_576 {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

#[test]
fn analyzer_is_silent_on_binary_accepted_configs() {
    let (tree, bin) = (
        common::required_env("OPENSIPS_LSP_TEST_TREE"),
        common::required_env("OPENSIPS_LSP_TEST_BIN"),
    );
    let mut files = corpus(std::path::Path::new(&tree));
    assert!(!files.is_empty(), "no corpus configs under {tree}");
    // the shipped examples may all be rejected on a box with a partial
    // module install; a synthetic baseline (verified-accepted
    // constructs, including past false-positive shapes: route(0),
    // include-defined targets, numeric routes) keeps the gate
    // non-vacuous everywhere
    let base_scratch = common::Scratch::new("oslsp-diff");
    let base = base_scratch.path().to_path_buf();
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(
        base.join("inc.cfg"),
        "route[from_include] {\n    exit;\n}\n",
    )
    .unwrap();
    for (name, text) in [
        (
            "baseline-basic.cfg",
            "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\nroute[named] {\n    exit;\n}\nroute[42] {\n    exit;\n}\nroute {\n    route(named);\n    route(42);\n    route(0);\n    exit;\n}\n",
        ),
        (
            "baseline-include.cfg",
            "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\ninclude_file \"inc.cfg\"\nroute {\n    route(from_include);\n    exit;\n}\n",
        ),
    ] {
        let p = base.join(name);
        std::fs::write(&p, text).unwrap();
        files.push(p);
    }
    let mut accepted = 0usize;
    for f in &files {
        let Ok(out) = std::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(f)
            .current_dir(f.parent().unwrap())
            .output()
        else {
            continue;
        };
        if out.status.code() != Some(0) {
            continue; // rejected configs are the corpus sweep's business
        }
        accepted += 1;
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let warns = logic::analyzer_diagnostics(f, &text, &disk_loader);
        assert!(
            warns.is_empty(),
            "{}: accepted by the real parser but the analyzer warns — \
             false positive:\n{warns:#?}",
            f.display()
        );
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(accepted > 0, "no config was accepted; gate is vacuous");
    eprintln!("analyzer silent on {accepted} binary-accepted configs");
}

#[test]
fn rename_round_trip_survives_the_real_parser() {
    let bin = common::required_env("OPENSIPS_LSP_TEST_BIN");
    let dir_scratch = common::Scratch::new("oslsp-rt");
    let dir = dir_scratch.path().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("rt.cfg");
    let original = "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\nroute[old_target] {\n    exit;\n}\nroute {\n    route(old_target);\n    exit;\n}\n";
    std::fs::write(&cfg, original).unwrap();
    let check = |p: &std::path::Path| {
        std::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(p)
            .current_dir(p.parent().unwrap())
            .output()
            .map(|o| o.status.code() == Some(0))
            .unwrap_or(false)
    };
    // a binary that rejects the baseline makes the round-trip prove
    // nothing, so it is a failure rather than a reason to opt out
    assert!(
        check(&cfg),
        "the test binary rejects the baseline config, so the rename \
         round-trip would prove nothing"
    );
    // rename through the REAL logic path: gate + occurrences
    let new_name = "relay_v2";
    assert!(logic::valid_route_name(new_name));
    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    let mut occ = logic::route_occurrences(original, "old_target");
    assert_eq!(occ.len(), 2, "def + one call site");
    // splice right-to-left so earlier columns stay valid
    occ.sort_by_key(|(l, _)| (l.line, std::cmp::Reverse(l.col)));
    for (l, _) in &occ {
        let line = &mut lines[l.line as usize];
        let (s, e) = (l.col as usize, l.col as usize + l.name.len());
        line.replace_range(s..e, new_name);
    }
    let renamed = lines.join("\n") + "\n";
    assert!(!renamed.contains("old_target"));
    assert!(renamed.matches(new_name).count() == 2);
    let out_cfg = dir.join("rt-renamed.cfg");
    std::fs::write(&out_cfg, &renamed).unwrap();
    assert!(
        check(&out_cfg),
        "the real parser rejects the renamed config:\n{renamed}"
    );
}

/// The README's own `modparam` examples, as ground truth for what the
/// catalogue must contain.
///
/// The harvester reads headings; the examples underneath them are
/// written by the same author for the same release, and they name the
/// parameter the way a configuration has to write it.  Where the two
/// disagree the harvester is normally the one that is wrong — that is
/// how the fenced-example truncation, the `#####` sub-heading, the
/// unspaced `db_url(str)` and the comma-listed heading were each
/// found, and every one of them made the server warn about a
/// parameter that exists.
///
/// The other direction — an example naming a parameter the module does
/// not export — used to be a hand-curated array of 32 triples. It is
/// now derived: a module's `param_export_t` tables are the list
/// `modparam()` is checked against at startup, so a name absent from
/// them is not a parameter, whoever wrote it. That is provable rather
/// than curated, and it carries its own evidence.
#[test]
fn the_catalogue_contains_every_parameter_the_readmes_set() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let root = std::path::Path::new(&tree);
    let re =
        regex::Regex::new(r#"modparam\(\s*"([A-Za-z0-9_]+)"\s*,\s*"([A-Za-z0-9_]+)""#).unwrap();
    let catalogue = opensips_lsp::catalog::builtin_modules();

    let mut checked = 0usize;
    let mut missing: Vec<String> = Vec::new();
    let mut not_exported: Vec<String> = Vec::new();
    let mut with_tables = 0usize;
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(root.join("modules"))
        .expect("an OpenSIPS source tree has modules/")
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let module = dir.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(readme) = std::fs::read_to_string(dir.join("README.md")) else {
            continue;
        };
        let Some(doc) = catalogue.modules.iter().find(|m| m.name == module) else {
            missing.push(format!(
                "{module}: the module itself is not in the catalogue"
            ));
            continue;
        };
        let exported = opensips_lsp::catalog::param_names_from_c(&dir, root);
        if !exported.names.is_empty() {
            with_tables += 1;
        }
        for c in re.captures_iter(&readme) {
            // only what the module says about ITSELF: a README often
            // shows another module's parameter in a worked example
            if c[1] != module {
                continue;
            }
            let param = c[2].to_string();
            checked += 1;
            if doc.params.iter().any(|p| p.name == param) {
                continue;
            }
            // absent from the catalogue is a failure unless the
            // module's own C tables say it is not a parameter at all
            if !exported.names.is_empty() && !exported.names.contains(&param) {
                let entry = format!("{module}::{param}");
                if !not_exported.contains(&entry) {
                    not_exported.push(entry);
                }
            } else {
                missing.push(format!("{module}::{param}"));
            }
        }
    }
    assert!(checked > 1500, "suspiciously few examples read: {checked}");
    // POSITIVE CONTROL. The exception is "absent from the C tables".
    // Were the extraction to stop matching, every example would be
    // excused and this test would pass having proven nothing.
    assert!(
        with_tables >= 175,
        "only {with_tables} modules exposed a parameter table; \
         the derived exception would excuse everything"
    );
    assert!(
        missing.is_empty(),
        "{} parameter(s) a README sets are exported by the module but absent from the catalogue:\n{}",
        missing.len(),
        missing.join("\n")
    );
    eprintln!(
        "catalogue covers {checked} README modparam examples; {} example(s) name a parameter the module never exported:\n{}",
        not_exported.len(),
        not_exported.join("\n")
    );
}

/// Every function name that appears at the top level of a heading,
/// such as `uac_replace_from([display],uri) uac_replace_to([display],uri)`:
/// an identifier followed by `(` outside any parentheses.
fn signature_names(heading: &str) -> Vec<String> {
    let b = heading.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            c if depth == 0 && (c.is_ascii_alphabetic() || c == b'_') => {
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                if b.get(i) == Some(&b'(') && (s == 0 || b[s - 1] != b'$') {
                    let name = heading[s..i].to_string();
                    if !out.contains(&name) {
                        out.push(name);
                    }
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// The functions a module's "Exported Functions" chapter names, read
/// from the source `harvest_tree` reads: `README.md` when it has the
/// chapter, the docbook admin guide otherwise.
fn documented_functions(dir: &std::path::Path, module: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(md) = std::fs::read_to_string(dir.join("README.md")) {
        let (mut inside, mut fence) = (false, false);
        for line in md.lines() {
            let t = line.trim_start();
            if t.starts_with("```") || t.starts_with("~~~") {
                fence = !fence;
                continue;
            }
            if fence || !line.starts_with('#') {
                continue;
            }
            let level = line.bytes().take_while(|&c| c == b'#').count();
            let title = line[level..].trim();
            if title == "Exported Functions" {
                inside = true;
            } else if level <= 3 {
                inside = false;
            } else if inside && level == 4 {
                out.extend(signature_names(title));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let Ok(xml) = std::fs::read_to_string(dir.join("doc").join(format!("{module}_admin.xml")))
    else {
        return out;
    };
    // entities are external to the file; their text is irrelevant here
    let xml = regex::Regex::new(r"&[A-Za-z0-9_.-]+;")
        .unwrap()
        .replace_all(&xml, "X");
    let xml = regex::Regex::new(r"(?s)<!DOCTYPE.*?\]>|<!DOCTYPE[^>]*>")
        .unwrap()
        .replace_all(&xml, "");
    let Ok(doc) = roxmltree::Document::parse(&xml) else {
        return out;
    };
    fn text(n: roxmltree::Node) -> String {
        n.descendants()
            .filter(|d| d.is_text())
            .filter_map(|d| d.text())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn title_of<'a, 'i>(n: roxmltree::Node<'a, 'i>) -> Option<roxmltree::Node<'a, 'i>> {
        n.children().find(|c| c.has_tag_name("title"))
    }
    for chapter in doc.descendants().filter(|n| n.has_tag_name("section")) {
        if title_of(chapter).map(text).as_deref() != Some("Exported Functions") {
            continue;
        }
        for s in chapter.children().filter(|c| c.has_tag_name("section")) {
            if let Some(t) = title_of(s) {
                out.extend(signature_names(&text(t)));
            }
        }
    }
    out
}

/// Every function a module's own "Exported Functions" chapter
/// documents is in the catalogue, for every pinned release.
///
/// The oracle reads the chapter independently of the harvester and
/// keeps only names the module's C sources contain as a string
/// literal, which is how a `cmd_export_t` entry names a function: a
/// name the documentation invents is not held against the catalogue.
/// It found 29 documented functions missing across the three
/// releases — two signatures under one heading, and docbook sections
/// whose id does not start with `func_`.
#[test]
fn the_catalogue_contains_every_function_the_docs_document() {
    let raw = common::required_env("OPENSIPS_LSP_TEST_TREES");
    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for pair in raw.split(',') {
        let (version, tree) = pair.split_once('=').expect("<version>=<tree>");
        let root = std::path::Path::new(tree);
        let catalogue = opensips_lsp::catalog::builtin_modules_at(version)
            .unwrap_or_else(|| panic!("{version} is a pinned release"));
        let harvested = opensips_lsp::catalog::harvest_tree(root);
        let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(root.join("modules"))
            .expect("an OpenSIPS source tree has modules/")
            .flatten()
            .map(|e| e.path())
            .collect();
        dirs.sort();
        for dir in dirs {
            let module = dir.file_name().unwrap().to_string_lossy().into_owned();
            let mut c_text = String::new();
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "c" || x == "h") {
                    c_text.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
                }
            }
            for f in documented_functions(&dir, &module) {
                if !c_text.contains(&format!("\"{f}\"")) {
                    continue;
                }
                checked += 1;
                for (what, mods) in [
                    ("vendored catalogue", &catalogue.modules),
                    ("harvest of the tree", &harvested),
                ] {
                    let found = mods
                        .iter()
                        .find(|m| m.name == module)
                        .is_some_and(|m| m.functions.iter().any(|x| x.name == f));
                    if !found {
                        missing.push(format!("{version} {what}: {module}::{f}"));
                    }
                }
            }
        }
    }
    // POSITIVE CONTROL: an oracle that stopped reading the docs would
    // pass having checked nothing
    assert!(checked > 1400, "suspiciously few functions read: {checked}");
    assert!(
        missing.is_empty(),
        "{} documented, exported function(s) absent:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

/// The other direction: every function in the catalogue is one its
/// module's C sources name, for every pinned release.
///
/// A docbook section's `id` is not always the function's name:
/// `func_get_dialog_by_val` documents `get_dialogs_by_val`, and
/// `func_call_blind_transfer` documents `call_transfer`. Harvested from
/// the id, the catalogue offered completions no configuration can
/// call. The name a caller writes is the one in the `<function>`
/// element of the section's title.
/// Upstream documentation errors: a function the docs name and the C
/// sources do not. Each is listed with its evidence, and each must
/// still fire: an entry that stopped firing would hide the next one.
const UPSTREAM_DOC_ERRORS: &[(&str, &str, &str, &str)] = &[
    (
        "3.6.9",
        "janus",
        "janus_send_requeest",
        "heading typo; janus_mod.c exports janus_send_request, as the README's own example writes it",
    ),
    (
        "4.0.2",
        "janus",
        "janus_send_requeest",
        "heading typo; janus_mod.c exports janus_send_request, as the README's own example writes it",
    ),
    (
        "3.6.9",
        "event_sqs",
        "sqs_publish_message",
        "documented name; event_sqs.c exports the function as sqs_send_message",
    ),
    (
        "4.0.2",
        "event_sqs",
        "sqs_publish_message",
        "documented name; event_sqs.c exports the function as sqs_send_message",
    ),
    (
        "4.0.2",
        "presence_callinfo",
        "sca_set_calling_line",
        "still documented; presence_callinfo.c exports only sca_set_called_line",
    ),
];

#[test]
fn no_catalogue_function_is_absent_from_its_modules_c_sources() {
    let raw = common::required_env("OPENSIPS_LSP_TEST_TREES");
    let mut phantom: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for pair in raw.split(',') {
        let (version, tree) = pair.split_once('=').expect("<version>=<tree>");
        let root = std::path::Path::new(tree);
        let catalogue = opensips_lsp::catalog::builtin_modules_at(version)
            .unwrap_or_else(|| panic!("{version} is a pinned release"));
        // the vendored catalogue, and what a configured tree yields at
        // runtime: the same harvester, read at two different times
        let harvested = opensips_lsp::catalog::harvest_tree(root);
        let mut found: Vec<String> = Vec::new();
        for (what, mods) in [("", &catalogue.modules), (" (harvest)", &harvested)] {
            for m in mods {
                let dir = root.join("modules").join(&m.name);
                let mut c_text = String::new();
                for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x == "c" || x == "h") {
                        c_text.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
                    }
                }
                if c_text.is_empty() {
                    continue;
                }
                for f in &m.functions {
                    checked += 1;
                    if !c_text.contains(&format!("\"{}\"", f.name)) {
                        found.push(format!("{version}{what} {}::{}", m.name, f.name));
                    }
                }
            }
        }
        for (v, module, f, why) in UPSTREAM_DOC_ERRORS {
            if *v != version {
                continue;
            }
            for what in ["", " (harvest)"] {
                let entry = format!("{version}{what} {module}::{f}");
                assert!(
                    found.contains(&entry),
                    "the documented exception {entry} ({why}) no longer fires: \
                     remove it from UPSTREAM_DOC_ERRORS"
                );
                found.retain(|p| *p != entry);
            }
        }
        phantom.extend(found);
    }
    assert!(checked > 2800, "suspiciously few functions read: {checked}");
    assert!(
        phantom.is_empty(),
        "{} catalogue function(s) the module's C sources never name:\n{}",
        phantom.len(),
        phantom.join("\n")
    );
}

/// The aliases the C sources define, read from every pinned tree: in
/// 3.5.9, 3.6.9 and 4.0.2 the only undocumented export sharing a
/// documented function's handler is sipmsgops `has_body`
/// (`has_body_f`, as `has_body_part`). Pinned as a set, so a new alias
/// upstream, or a reading that finds none, fails here.
#[test]
fn the_catalogue_carries_exactly_the_aliases_the_c_sources_define() {
    let raw = common::required_env("OPENSIPS_LSP_TEST_TREES");
    for pair in raw.split(',') {
        let (version, tree) = pair.split_once('=').expect("<version>=<tree>");
        let vendored = opensips_lsp::catalog::builtin_modules_at(version)
            .unwrap_or_else(|| panic!("{version} is a pinned release"));
        let harvested = opensips_lsp::catalog::harvest_tree(std::path::Path::new(tree));
        for (what, mods) in [("vendored", &vendored.modules), ("harvest", &harvested)] {
            let aliases: Vec<String> = mods
                .iter()
                .flat_map(|m| {
                    m.functions
                        .iter()
                        .filter(|f| f.doc.starts_with("Alias of `"))
                        .map(move |f| format!("{}::{} {}", m.name, f.name, f.detail))
                })
                .collect();
            assert_eq!(
                aliases,
                vec!["sipmsgops::has_body has_body([mime])".to_string()],
                "{version} {what}"
            );
        }
    }
}
