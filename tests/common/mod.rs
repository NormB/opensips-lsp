//! Shared stdio-LSP test harness.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub fn write_msg(w: &mut impl Write, v: &serde_json::Value) {
    let s = v.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{}", s.len(), s).unwrap();
    w.flush().unwrap();
}

pub fn spawn_reader(child: &mut Child) -> mpsc::Receiver<serde_json::Value> {
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        loop {
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let t = line.trim();
                if t.is_empty() {
                    break;
                }
                if let Some(v) = t.strip_prefix("Content-Length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut buf = vec![0u8; len];
            if r.read_exact(&mut buf).is_err() {
                return;
            }
            if let Ok(v) = serde_json::from_slice(&buf)
                && tx.send(v).is_err()
            {
                return;
            }
        }
    });
    rx
}

pub fn wait_for<F: Fn(&serde_json::Value) -> bool>(
    rx: &mpsc::Receiver<serde_json::Value>,
    pred: F,
    what: &str,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("timeout waiting for {what}"));
        let v = rx
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("timeout waiting for {what}"));
        if pred(&v) {
            return v;
        }
    }
}

/// The proof environment is REQUIRED, not optional: under this
/// project's rule a skipped test IS a failed test.  A suite that
/// quietly opts out on a machine without a real tree or binary is a
/// suite that reports green while proving nothing.
///
/// `scripts/proof-env.sh` provisions what these tests need, and CI
/// runs that same script — the gate and its fixer derive from one
/// rule, so a green CI means the proofs actually ran.
pub fn required_env(var: &str) -> String {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => v,
        _ => panic!(
            "{var} is unset or empty.  These tests run against a real \
             tree and binary and are never skipped: run \
             `scripts/proof-env.sh` and export what it prints (CI runs \
             the same script)."
        ),
    }
}

/// A spawned server that owns the fixture directory it was given.
///
/// Every stdio test used to kill its child and remove its directory on
/// the last line of the test body — which is the one line a failing
/// assertion never reaches.  A green run left 21 directories behind; a
/// red one also left a server process running, and one of those was
/// found alive nearly six hours later.  Cleanup that only happens when
/// nothing goes wrong is not cleanup, so it belongs in `Drop`, which
/// runs on the panic path too.
///
/// `Deref` keeps every existing call site working: `spawn_reader(&mut
/// child)` and `child.stdin.take()` still mean what they meant, and an
/// explicit `child.kill()` at the end of a test is now belt-and-braces
/// rather than the only thing standing between a failure and a leak.
pub struct Server {
    child: Child,
    dir: PathBuf,
}

impl Server {
    pub fn new(child: Child, dir: impl AsRef<Path>) -> Self {
        Self {
            child,
            dir: dir.as_ref().to_path_buf(),
        }
    }

    /// Shadows `Child::kill` so the existing `let _ = child.kill();`
    /// lines keep compiling and keep meaning "stop it now".
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }
}

impl Deref for Server {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}

impl DerefMut for Server {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        // reap it, or the process lingers as a zombie for as long as
        // the test binary runs
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Every `.cfg` under `dir`, recursively, appended to `out`.
pub fn cfgs_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            cfgs_under(&p, out);
        } else if p.extension().is_some_and(|x| x == "cfg") {
            out.push(p);
        }
    }
}

/// `opensips -C` on `f`, run from its directory as the server runs it;
/// true when the parser reported a syntax error.
pub fn syntax_error(bin: &str, f: &Path) -> bool {
    let out = Command::new(bin)
        .arg("-C")
        .arg("-f")
        .arg(f)
        .current_dir(f.parent().unwrap())
        .output()
        .expect("spawn opensips -C");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    all.contains("syntax error")
}

/// One corpus case: (title, input, expected-tree text).
pub fn parse_corpus(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        if !l.starts_with("====") {
            continue;
        }
        let title = lines.next().unwrap_or("").trim().to_string();
        // closing ==== of the header
        for l in lines.by_ref() {
            if l.starts_with("====") {
                break;
            }
        }
        let mut input = String::new();
        for l in lines.by_ref() {
            if l.starts_with("----") {
                break;
            }
            input.push_str(l);
            input.push('\n');
        }
        let mut expected = String::new();
        while let Some(l) = lines.peek() {
            if l.starts_with("====") {
                break;
            }
            expected.push_str(lines.next().unwrap());
            expected.push('\n');
        }
        out.push((title, input, expected));
    }
    out
}

/// A scratch path under the system temp directory, removed when the
/// guard is dropped.
///
/// Cleanup written as the last statement of a test runs only when every
/// assertion before it held, so a failing test left its directory
/// behind. `Drop` also runs while a panic unwinds, so this removes the
/// path whether the test passes, fails an assertion or panics.
///
/// The tests of one binary are threads of one process, so `name` must
/// be unique within the binary; the process id is appended. Nothing is
/// created: the path is reserved, and anything stale left at it is
/// removed. Hold the guard for as long as the path is used.
///
/// Two live guards on one path panic. A helper that clears "its" path
/// while a test still holds the same one deletes the test's fixture,
/// and two tests running in parallel on one path clobber each other;
/// both happened here, and both passed silently.
pub struct Scratch(PathBuf);

static LIVE: std::sync::Mutex<BTreeSet<PathBuf>> = std::sync::Mutex::new(BTreeSet::new());

impl Scratch {
    pub fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let fresh = LIVE
            .lock()
            .expect("poisoned: a thread panicked while holding the set of live scratch paths")
            .insert(path.clone());
        assert!(
            fresh,
            "{} is already held by another live Scratch: give each test and helper its own name",
            path.display()
        );
        remove_path(&path);
        Scratch(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        remove_path(&self.0);
        LIVE.lock()
            .expect("poisoned: a thread panicked while holding the set of live scratch paths")
            .remove(&self.0);
    }
}

fn remove_path(path: &Path) {
    if std::fs::remove_dir_all(path).is_err() {
        let _ = std::fs::remove_file(path);
    }
}

/// The tree-sitter CLI the grammar tests run, as `npx` names it.
pub const TREE_SITTER_CLI: &str = "tree-sitter-cli@0.26";

/// The file, in the CLI's cache directory, that [`tree_sitter`] locks
/// for as long as a CLI it started runs.
pub const TREE_SITTER_GUARD: &str = "tree-sitter-cli.callers.lock";

/// The directory the tree-sitter CLI keeps its compiled grammars and
/// its lock files under, as the CLI itself chooses it.
fn tree_sitter_cache_home() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        _ => PathBuf::from(required_env("HOME")).join(".cache"),
    }
}

/// Runs `npx tree-sitter-cli <args>` in `dir` and gives its output.
/// `cache_home` replaces the CLI's cache directory (`XDG_CACHE_HOME`)
/// for this call.
///
/// Every test goes through here, because two CLI processes must not
/// load one grammar at the same time on a cache that has not compiled
/// it yet. The CLI compiles the grammar into its cache behind a lock
/// file of its own, `<cache>/tree-sitter/lock/<grammar>.lock`, and that
/// lock races: started together, one process removes the file while
/// another opens it, and the second ends with "Failed to load language
/// ... No such file or directory ... .lock". The tests of one binary
/// are threads that each start a CLI, and main's CI failed on it on
/// 2026-11-14.
///
/// So each call holds an exclusive lock on [`TREE_SITTER_GUARD`] in the
/// cache directory while its CLI runs: one CLI at a time per cache,
/// across the threads of a test binary and across processes (another
/// test binary, another checkout on the same machine). The first call
/// on an empty cache compiles the grammar alone, and the rest load it.
pub fn tree_sitter<I, S>(dir: &Path, cache_home: Option<&Path>, args: I) -> std::process::Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let cache = cache_home.map_or_else(tree_sitter_cache_home, Path::to_path_buf);
    std::fs::create_dir_all(&cache).expect("the tree-sitter cache directory must exist");
    let guard = std::fs::File::options()
        .create(true)
        .append(true)
        .open(cache.join(TREE_SITTER_GUARD))
        .expect("the guard file in the tree-sitter cache directory");
    // Released when `guard` is closed at the end of this call.
    guard
        .lock()
        .expect("an exclusive lock on the tree-sitter guard file");
    let mut cli = Command::new("npx");
    cli.args(["--yes", TREE_SITTER_CLI])
        .args(args)
        .current_dir(dir);
    if let Some(cache) = cache_home {
        cli.env("XDG_CACHE_HOME", cache);
    }
    cli.output()
        .expect("npx must be able to run the tree-sitter CLI")
}
