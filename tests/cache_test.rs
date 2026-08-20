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
