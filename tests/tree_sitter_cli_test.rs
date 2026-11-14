//! The tree-sitter CLI is run by several tests at once, and on a cache
//! that has not compiled the grammar yet its own lock file races: one
//! process removes `<cache>/tree-sitter/lock/<grammar>.lock` while
//! another opens it, and that one ends with "Failed to load language".
//! `common::tree_sitter` runs one CLI at a time per cache; this holds it
//! to that, on caches made empty for the purpose.

mod common;

use std::ffi::OsStr;
use std::path::Path;

/// How many CLI calls each round starts together.
const CALLS: usize = 6;
/// How many empty caches the calls are started on, one after another.
const ROUNDS: usize = 2;
/// How long a call is given to finish while another holder has the
/// guard. A `--version` call takes well under a second on its own.
const HELD: std::time::Duration = std::time::Duration::from_secs(4);

/// The race itself needs two CLI processes to reach the lock file
/// within microseconds of each other, so the test below shows it only
/// now and then: it was seen in CI and in one of five rounds of a shell
/// loop, and in none of some fifty rounds of that test run without the
/// guard. This one is exact: while anything else holds the cache's
/// guard, in this process or another, a call does not run.
#[test]
fn a_call_waits_while_another_holder_has_the_guard_of_its_cache() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let grammar = root.join("tree-sitter-opensips");
    let cache = common::Scratch::new("oslsp-ts-guard-held");
    std::fs::create_dir_all(cache.path()).expect("the cache directory");
    // An open file of its own, as another process would have.
    let holder = std::fs::File::options()
        .create(true)
        .append(true)
        .open(cache.path().join(common::TREE_SITTER_GUARD))
        .expect("the guard file");
    holder.lock().expect("the guard is free at the start");

    let (done, finished) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let out = common::tree_sitter(&grammar, Some(cache.path()), ["--version"]);
            done.send(out.status.success())
                .expect("the test is still listening");
        });
        assert!(
            finished.recv_timeout(HELD).is_err(),
            "the CLI ran while another holder had the guard of its cache"
        );
        holder.unlock().expect("the guard can be released");
        assert_eq!(
            finished.recv_timeout(std::time::Duration::from_secs(120)),
            Ok(true),
            "the call must run once the guard is free"
        );
    });
}

#[test]
fn calls_started_together_on_an_empty_cache_all_load_the_grammar() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let grammar = root.join("tree-sitter-opensips");
    let file = root.join("tests/grammar/constructs.cfg");
    for round in 0..ROUNDS {
        let cache = common::Scratch::new(&format!("oslsp-ts-empty-cache-{round}"));
        std::fs::create_dir_all(cache.path()).expect("the empty cache directory");
        let outputs: Vec<std::process::Output> = std::thread::scope(|scope| {
            let calls: Vec<_> = (0..CALLS)
                .map(|_| {
                    scope.spawn(|| {
                        common::tree_sitter(
                            &grammar,
                            Some(cache.path()),
                            [OsStr::new("parse"), OsStr::new("--quiet"), file.as_os_str()],
                        )
                    })
                })
                .collect();
            calls
                .into_iter()
                .map(|call| call.join().expect("a CLI call's thread"))
                .collect()
        });
        // POSITIVE CONTROL: the calls must have compiled the grammar
        // into this cache, or they did not use it and proved nothing.
        assert!(
            cache.path().join("tree-sitter").is_dir(),
            "round {round}: the CLI did not use the cache directory it was given"
        );
        for (call, out) in outputs.iter().enumerate() {
            assert!(
                out.status.success(),
                "round {round}, call {call} of {CALLS} started together failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

/// The guard is one lock per cache, so calls on two caches do not wait
/// for each other's lock file, and each cache gets its own.
#[test]
fn the_guard_is_kept_beside_the_cache_it_guards() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let grammar = root.join("tree-sitter-opensips");
    let cache = common::Scratch::new("oslsp-ts-guard-place");
    std::fs::create_dir_all(cache.path()).expect("the cache directory");
    let out = common::tree_sitter(&grammar, Some(cache.path()), ["--version"]);
    assert!(
        out.status.success(),
        "tree-sitter --version: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        cache.path().join(common::TREE_SITTER_GUARD).is_file(),
        "the guard file must be in the cache directory the call used"
    );
}
