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

    // change the tree: new module directory => new fingerprint => miss
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::create_dir_all(tree.join("modules").join("new_mod")).unwrap();
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
