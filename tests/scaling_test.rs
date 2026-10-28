//! Cost must grow with the size of the config, not with its square.
//!
//! Every scanner used to turn each match offset into a (line, column)
//! by recounting newlines from the start of the text, so a file twice
//! as long took four times as long to scan: `route_blocks` alone took
//! 73 s on the config below.  These tests time the scanners on
//! that config against a budget the linear version meets with a wide
//! margin on a loaded host.

use opensips_lsp::analyze;
use std::time::{Duration, Instant};

/// `n` chained request routes, each with a call, a pvar, a modparam
/// and a route reference: every scanner has thousands of matches to
/// place.
fn big_cfg(n: usize) -> String {
    let mut s = String::from("loadmodule \"tm.so\"\nloadmodule \"sl.so\"\n");
    for i in 0..n {
        s.push_str(&format!(
            "modparam(\"tm\", \"fr_timeout\", {i})\n\
             route[r{i}] {{\n    xlog(\"L_INFO\", \"$ru $si {i}\");\n    route(r{});\n}}\n",
            i + 1
        ));
    }
    s
}

const N: usize = 6000;

/// The linear scanners finish this in well under a second in a debug
/// build; the quadratic ones needed tens of seconds.
const BUDGET: Duration = Duration::from_secs(5);

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    let took = t.elapsed();
    assert!(
        took < BUDGET,
        "{what} took {took:?} on a {N}-route config (budget {BUDGET:?})"
    );
    out
}

#[test]
fn scanners_are_linear_in_the_text() {
    let cfg = big_cfg(N);
    assert_eq!(
        timed("route_blocks", || analyze::route_blocks(&cfg)).len(),
        N
    );
    assert_eq!(timed("route_refs", || analyze::route_refs(&cfg)).len(), N);
    assert!(timed("calls", || analyze::calls(&cfg)).len() >= 2 * N);
    assert_eq!(
        timed("loaded_modules", || analyze::loaded_modules(&cfg)).len(),
        2
    );
    assert_eq!(
        timed("modparam_calls", || analyze::modparam_calls(&cfg)).len(),
        N
    );
    assert_eq!(timed("pvars", || analyze::pvars(&cfg)).len(), 2 * N);
    assert!(timed("includes", || analyze::includes(&cfg)).is_empty());
}

#[test]
fn positions_at_the_end_of_a_large_config_are_exact() {
    // the fast path must place the LAST match exactly where the slow
    // one did: line 2 + 5 lines per route, `route(` indented 4
    let cfg = big_cfg(N);
    let last = analyze::route_refs(&cfg).pop().unwrap();
    assert_eq!(last.name, format!("r{N}"));
    assert_eq!(last.line, 2 + 5 * (N as u32 - 1) + 3);
    assert_eq!(last.col, 10);
    let blk = analyze::route_blocks(&cfg).pop().unwrap();
    assert_eq!((blk.line, blk.col), (2 + 5 * (N as u32 - 1) + 1, 0));
    assert_eq!((blk.end_line, blk.end_col), (2 + 5 * (N as u32 - 1) + 4, 1));
}
