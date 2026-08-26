//! Catalog cache: harvest results are cached per source tree and
//! invalidated when the tree changes.

use opensips_lsp::catalog::{CoreDocs, Item, ModuleDoc, load_cached, save_cache, tree_fingerprint};

fn mk_tree(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("oslsp-cache-tree-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("modules").join("m")).unwrap();
    std::fs::create_dir_all(root.join("docs").join("manual")).unwrap();
    root
}

fn sample() -> (Vec<ModuleDoc>, CoreDocs) {
    (
        vec![ModuleDoc {
            name: "tm".into(),
            params: vec![Item {
                name: "fr_timeout".into(),
                detail: "integer".into(),
                doc: "d".into(),
            }],
            functions: vec![],
        }],
        CoreDocs {
            functions: vec![Item {
                name: "cache_store".into(),
                detail: "sig".into(),
                doc: "d".into(),
            }],
            params: vec![],
            pvars: vec![],
            statements: vec![],
            routes: vec![],
            socket_modifiers: vec![],
            log_levels: vec![],
        },
    )
}

#[test]
fn cache_roundtrips_and_invalidates_on_tree_change() {
    let tree = mk_tree("rt");
    let cache = std::env::temp_dir().join(format!("oslsp-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let (mods, core) = sample();

    // nothing cached yet
    assert!(load_cached(&tree, &cache).is_none());

    save_cache(&tree, &cache, &mods, &core).expect("save");
    let (m2, c2) = load_cached(&tree, &cache).expect("hit");
    assert_eq!(m2, mods);
    assert_eq!(c2.functions[0].name, "cache_store");

    // an EMPTY new module dir harvests nothing — still a valid cache
    std::fs::create_dir_all(tree.join("modules").join("new_mod")).unwrap();
    assert!(
        load_cached(&tree, &cache).is_some(),
        "an empty module dir changes nothing harvestable"
    );
    // a new module WITH docs => new fingerprint => miss
    std::fs::write(
        tree.join("modules").join("new_mod").join("README.md"),
        "### Exported Parameters\n\n#### x (string)\n\nDoc.\n",
    )
    .unwrap();
    assert!(
        load_cached(&tree, &cache).is_none(),
        "stale cache must miss after the tree changes"
    );

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn corrupt_cache_is_a_miss_not_a_panic() {
    let tree = mk_tree("corrupt");
    let cache = std::env::temp_dir().join(format!("oslsp-cache-c-{}", std::process::id()));
    std::fs::create_dir_all(&cache).unwrap();
    let fp = tree_fingerprint(&tree);
    std::fs::write(cache.join(format!("{fp}.json")), b"{not json").unwrap();
    assert!(load_cached(&tree, &cache).is_none());
    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn distinct_trees_have_distinct_fingerprints() {
    let a = mk_tree("a");
    let b = mk_tree("b");
    assert_ne!(tree_fingerprint(&a), tree_fingerprint(&b));
    let _ = std::fs::remove_dir_all(&a);
    let _ = std::fs::remove_dir_all(&b);
}

#[test]
fn cache_writes_are_atomic_and_leave_no_temp_files() {
    let tree = mk_tree("atomic");
    let cache = std::env::temp_dir().join(format!("oslsp-cache-a-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let (mods, core) = sample();

    // hammer: concurrent writers and readers; a torn write would make
    // a reader parse a partial file (load must be Some(valid) or None,
    // and must never panic)
    let tree2 = tree.clone();
    let cache2 = cache.clone();
    let writer = std::thread::spawn(move || {
        for _ in 0..200 {
            save_cache(&tree2, &cache2, &mods, &core).unwrap();
        }
    });
    for _ in 0..200 {
        if let Some((m, _)) = load_cached(&tree, &cache) {
            assert_eq!(m[0].name, "tm");
        }
    }
    writer.join().unwrap();

    // after the dust settles: exactly the final artifact, no temp litter
    let leftovers: Vec<String> = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with(".json"))
        .collect();
    assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");

    // a stray temp file from a crashed writer must be ignored by load
    std::fs::write(cache.join("deadbeef.json.tmp"), b"{torn").unwrap();
    assert!(load_cached(&tree, &cache).is_some());

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn editing_a_harvested_files_content_invalidates_the_cache() {
    // directory mtimes do NOT change when an existing file is edited
    // in place — the fingerprint must track the harvested files
    // themselves
    let tree = mk_tree("edit");
    std::fs::write(
        tree.join("modules").join("m").join("README.md"),
        "### Exported Parameters\n\n#### p1 (string)\n\nOld doc.\n",
    )
    .unwrap();
    let cache = std::env::temp_dir().join(format!("oslsp-cache-e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let (mods, core) = sample();
    save_cache(&tree, &cache, &mods, &core).expect("save");
    assert!(load_cached(&tree, &cache).is_some(), "warm hit");

    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(
        tree.join("modules").join("m").join("README.md"),
        "### Exported Parameters\n\n#### p2 (string)\n\nNew doc.\n",
    )
    .unwrap();
    assert!(
        load_cached(&tree, &cache).is_none(),
        "in-place edit of a harvested file must miss"
    );

    // creating a previously-absent doc file invalidates too
    save_cache(&tree, &cache, &mods, &core).expect("resave");
    assert!(load_cached(&tree, &cache).is_some());
    std::fs::create_dir_all(tree.join("modules").join("m").join("doc")).unwrap();
    std::fs::write(
        tree.join("modules")
            .join("m")
            .join("doc")
            .join("m_admin.xml"),
        "<chapter/>",
    )
    .unwrap();
    assert!(
        load_cached(&tree, &cache).is_none(),
        "a new harvested file must miss"
    );

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&cache);
}

/// Editing the routes page must invalidate the cache.
///
/// The fingerprint lists every file the harvest reads, and
/// `Script-Routes.md` was added to that harvest without being added
/// to the list. A cache written before an edit to it would keep being
/// served afterwards, so a corrected route description would never
/// reach the reader — and a cache written by a build that did not
/// harvest routes at all would serve none, silently.
#[test]
fn editing_the_routes_page_invalidates_the_cache() {
    let tree = mk_tree("routes");
    let page = tree.join("docs").join("manual").join("Script-Routes.md");
    std::fs::write(&page, "## startup_route\n\nBefore.\n").unwrap();
    let before = tree_fingerprint(&tree);

    // Same length, different content: size alone cannot see this, so
    // the manifest carries mtime — and mtime has a granularity, which
    // is why the sleep is here rather than being flaky without it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&page, "## startup_route\n\nAfterX.\n").unwrap();
    let after = tree_fingerprint(&tree);

    assert_ne!(
        before, after,
        "an edit to Script-Routes.md must change the fingerprint"
    );
    let _ = std::fs::remove_dir_all(&tree);
}

/// Every manual page the core harvest reads must be in the
/// fingerprint's file list.
///
/// `Script-Routes.md` was added to the harvest and not to that list,
/// so an edit to it left a warm cache serving the old text. Naming
/// the pages twice — once to read, once to fingerprint — is what
/// allowed the two to drift, so this derives one list from the other
/// rather than restating it. The next page added is covered without
/// anyone remembering.
#[test]
fn every_page_the_harvest_reads_is_fingerprinted() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/catalog.rs"))
        .expect("the source is readable");

    let harvest = src
        .split_once("pub fn harvest_core")
        .map(|(_, rest)| rest.split_once("\n}").map(|(b, _)| b).unwrap_or(rest))
        .expect("harvest_core exists");
    let read_pages: Vec<String> = harvest
        .match_indices("read(\"")
        .filter_map(|(i, _)| harvest[i + 6..].split_once('"').map(|(n, _)| n.to_string()))
        .collect();

    let fingerprint = src
        .split_once("pub fn tree_fingerprint")
        .map(|(_, rest)| rest)
        .expect("tree_fingerprint exists");
    let listed: Vec<String> = fingerprint
        .match_indices(".md\"")
        .filter_map(|(i, _)| {
            let start = fingerprint[..i].rfind('"')? + 1;
            Some(fingerprint[start..i + 3].to_string())
        })
        .collect();

    // POSITIVE CONTROL: a scan that stopped matching would find no
    // pages and agree with an empty list, proving nothing.
    assert!(
        read_pages.len() >= 4,
        "only {} page(s) found in harvest_core: {read_pages:?}",
        read_pages.len()
    );
    for page in &read_pages {
        assert!(
            listed.contains(page),
            "{page} is harvested but absent from the fingerprint — \
             an edit to it would leave a stale cache. Listed: {listed:?}"
        );
    }
}

/// The schema version must be part of what the fingerprint hashes.
///
/// The routes field arrived with `#[serde(default)]`, so a cache
/// written before routes were harvested still deserializes — with an
/// empty route list. Served, it answers no route hovers at all and
/// nothing about it looks broken. Bumping the schema is what turns
/// that into a miss, and it only works if the version is mixed into
/// the hash. It cannot be checked by editing a stored fingerprint,
/// because the stored value IS a hash; so this checks the input.
#[test]
fn the_fingerprint_hashes_the_schema_version() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/catalog.rs"))
        .expect("the source is readable");
    let body = src
        .split_once("pub fn tree_fingerprint")
        .map(|(_, rest)| rest.split_once("\n}").map(|(b, _)| b).unwrap_or(rest))
        .expect("tree_fingerprint exists");

    assert!(
        body.contains("CACHE_SCHEMA_VERSION"),
        "the schema version must be mixed into the fingerprint, or bumping it \
         leaves every stale cache being served"
    );
    // POSITIVE CONTROL: a scan that grabbed the wrong span would find
    // nothing and pass the moment the assertion above was inverted.
    assert!(
        body.contains("files.sort()"),
        "the scan did not capture tree_fingerprint's body"
    );
    // and the constant must actually have moved past the schema that
    // predates the routes harvest
    let version: u32 = src
        .split_once("const CACHE_SCHEMA_VERSION: u32 = ")
        .and_then(|(_, r)| r.split_once(';'))
        .and_then(|(v, _)| v.trim().parse().ok())
        .expect("the constant is readable");
    assert!(
        version >= 3,
        "routes were added to the harvest at schema 3; found {version}"
    );
}
