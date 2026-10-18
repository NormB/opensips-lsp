//! Shared stdio-LSP test harness.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::Child;
use std::sync::mpsc;
use std::time::Duration;

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
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline
            .checked_duration_since(std::time::Instant::now())
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
    dir: std::path::PathBuf,
}

impl Server {
    pub fn new(child: Child, dir: impl AsRef<std::path::Path>) -> Self {
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

impl std::ops::Deref for Server {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}

impl std::ops::DerefMut for Server {
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
pub fn cfgs_under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
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
pub fn syntax_error(bin: &str, f: &std::path::Path) -> bool {
    let out = std::process::Command::new(bin)
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
