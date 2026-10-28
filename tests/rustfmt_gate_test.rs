//! The rustfmt gate is wired: CI formats with the toolchain that
//! `scripts/fmt.sh` pins, and `rustfmt.toml` holds the Google Rust
//! Style Guide's settings.
//!
//! Three of those settings are unstable, so a stable rustfmt ignores them
//! and passes. A CI step that ran `cargo fmt` on the stable toolchain, or a
//! toolchain pinned in two places that drift apart, would leave the
//! settings unchecked while the job stayed green.

use std::fs;

fn read(rel: &str) -> String {
    let path = format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path} is not readable: {e}"))
}

#[test]
fn rustfmt_toml_holds_the_google_settings() {
    let toml = read("rustfmt.toml");
    let settings: Vec<&str> = toml
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    for wanted in [
        "style_edition = \"2024\"",
        "max_width = 100",
        "hex_literal_case = \"Lower\"",
        "format_macro_matchers = true",
        "imports_granularity = \"Module\"",
    ] {
        assert!(
            settings.contains(&wanted),
            "rustfmt.toml lacks `{wanted}`; it has {settings:?}"
        );
    }
}

#[test]
fn the_script_pins_a_dated_nightly() {
    let script = read("scripts/fmt.sh");
    let pins: Vec<&str> = script
        .lines()
        .filter_map(|l| l.strip_prefix("TOOLCHAIN="))
        .collect();
    assert_eq!(
        pins.len(),
        1,
        "scripts/fmt.sh must set TOOLCHAIN once: {pins:?}"
    );
    let date = pins[0].strip_prefix("nightly-").unwrap_or_else(|| {
        panic!("the pin {} is not a nightly", pins[0]);
    });
    let parts: Vec<&str> = date.split('-').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()),
        "the pin {} is not dated as nightly-YYYY-MM-DD",
        pins[0]
    );
}

#[test]
fn ci_formats_with_the_pinned_toolchain() {
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("rustup toolchain install \"$(scripts/fmt.sh toolchain)\""),
        "ci.yml must install the toolchain scripts/fmt.sh names"
    );
    assert!(
        ci.contains("run: scripts/fmt.sh check")
            || ci.lines().any(|l| l.trim() == "scripts/fmt.sh check"),
        "ci.yml must run scripts/fmt.sh check"
    );
    let stable_fmt: Vec<&str> = ci
        .lines()
        .filter(|l| l.contains("cargo fmt") || l.contains("cargo +stable fmt"))
        .collect();
    assert!(
        stable_fmt.is_empty(),
        "ci.yml runs rustfmt outside the script, where the unstable settings \
         are ignored: {stable_fmt:?}"
    );
}
