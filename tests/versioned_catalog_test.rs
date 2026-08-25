//! Base plus forward deltas must reproduce every release exactly.
//!
//! The catalogue ships one release in full and a delta per later one,
//! because releases resemble each other far more than they differ —
//! shipping each whole would be mostly duplicated bytes, and the
//! duplication would grow with every release added. That only holds
//! if applying the deltas is lossless, which is what these prove
//! against the real trees rather than against fixtures.

mod common;

use opensips_lsp::catalog::{
    BuiltinModules, ModuleDoc, VersionedModules, canonicalize, diff_catalogues, harvest_tree,
};

/// `<version>=<path>` pairs, oldest first, from `proof-env.sh`.
fn trees() -> Vec<(String, std::path::PathBuf)> {
    let raw = common::required_env("OPENSIPS_LSP_TEST_TREES");
    raw.split(',')
        .filter_map(|entry| {
            let (v, p) = entry.split_once('=')?;
            Some((v.to_string(), std::path::PathBuf::from(p)))
        })
        .collect()
}

fn harvests() -> Vec<(String, Vec<ModuleDoc>)> {
    trees()
        .into_iter()
        .map(|(v, p)| {
            let mut mods = harvest_tree(&p);
            // the versioned catalogue is stored canonically, so a
            // harvest must be put in the same order before the two
            // can be compared for equality at all
            canonicalize(&mut mods);
            (v, mods)
        })
        .collect()
}

fn build(harvests: &[(String, Vec<ModuleDoc>)]) -> VersionedModules {
    let (base_v, base_m) = &harvests[0];
    let mut deltas = Vec::new();
    for pair in harvests.windows(2) {
        deltas.push(diff_catalogues(&pair[0].1, &pair[1].1, &pair[1].0));
    }
    VersionedModules {
        base: BuiltinModules {
            version: base_v.clone(),
            modules: base_m.clone(),
        },
        deltas,
    }
}

/// POSITIVE CONTROL. If every release harvested the same, the
/// round-trip below would hold no matter how broken the delta logic
/// was. It must be shown that the releases actually differ before
/// their reconstruction proves anything.
#[test]
fn the_releases_actually_differ() {
    let h = harvests();
    assert!(
        h.len() >= 3,
        "expected the three live lines, got {:?}",
        h.iter().map(|(v, _)| v).collect::<Vec<_>>()
    );
    for (v, mods) in &h {
        assert!(
            mods.len() > 150,
            "{v}: only {} modules harvested",
            mods.len()
        );
    }
    let mut differing = 0usize;
    for pair in h.windows(2) {
        if pair[0].1 != pair[1].1 {
            differing += 1;
        }
    }
    assert_eq!(
        differing,
        h.len() - 1,
        "every adjacent pair of releases must differ, or the round-trip is vacuous"
    );
}

/// The property the whole format rests on.
#[test]
fn base_plus_deltas_reproduces_every_release() {
    let h = harvests();
    let versioned = build(&h);

    for (version, expected) in &h {
        let got = versioned
            .at(version)
            .unwrap_or_else(|| panic!("{version} must be a supported release"));
        assert_eq!(
            got.len(),
            expected.len(),
            "{version}: reconstructed {} modules, harvested {}",
            got.len(),
            expected.len()
        );
        // compare module by module so a failure names the module
        for (a, b) in got.iter().zip(expected.iter()) {
            assert_eq!(a.name, b.name, "{version}: module order diverged");
            assert_eq!(
                a, b,
                "{version}: module '{}' does not match a fresh harvest",
                a.name
            );
        }
        assert_eq!(&got, expected, "{version}: reconstruction is not exact");
    }
}

#[test]
fn versions_are_listed_oldest_first_and_newest_is_last() {
    let h = harvests();
    let versioned = build(&h);
    let expected: Vec<String> = h.iter().map(|(v, _)| v.clone()).collect();
    assert_eq!(versioned.versions(), expected);
    assert_eq!(versioned.newest(), expected.last().unwrap());
}

#[test]
fn an_unsupported_release_resolves_to_nothing() {
    let versioned = build(&harvests());
    assert!(versioned.at("1.2.3").is_none());
    assert!(versioned.at("").is_none());
}

/// The lookup behind cross-version diagnosis, checked against the
/// harvests rather than against itself: for a parameter that really
/// did arrive between releases, the versions reported must be exactly
/// the versions whose own harvest contains it.
#[test]
fn versions_with_param_agrees_with_the_harvests() {
    let h = harvests();
    let versioned = build(&h);

    let has = |mods: &[ModuleDoc], m: &str, p: &str| {
        mods.iter()
            .any(|d| d.name == m && d.params.iter().any(|x| x.name == p))
    };
    // a parameter present in the newest release and absent from the
    // oldest — the shape a version mismatch actually takes
    let (_, newest) = h.last().unwrap();
    let (_, oldest) = h.first().unwrap();
    let mut found = None;
    for m in newest {
        for p in &m.params {
            if !has(oldest, &m.name, &p.name) {
                found = Some((m.name.clone(), p.name.clone()));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (module, param) =
        found.expect("some parameter must have arrived between the oldest and newest release");

    let expected: Vec<String> = h
        .iter()
        .filter(|(_, mods)| has(mods, &module, &param))
        .map(|(v, _)| v.clone())
        .collect();
    assert_eq!(
        versioned.versions_with_param(&module, &param),
        expected,
        "{module}::{param}"
    );
    assert!(
        !expected.is_empty() && expected.len() < h.len(),
        "{module}::{param} must be in some releases but not all, got {expected:?}"
    );
}
