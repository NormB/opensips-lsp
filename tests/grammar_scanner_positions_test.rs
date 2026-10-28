//! The tree-sitter grammar and the server's scanner must find the same
//! constructs at the same positions, in every configuration the real
//! parser accepts.
//!
//! Editors other than VS Code (Neovim, Helix, Zed) take a config's
//! structure from the grammar; the server takes it from the scanner in
//! `analyze`.  Where the two disagree, the outline an editor shows and
//! the definitions the server navigates to describe different files.
//!
//! `grammar_scanner_drift_test.rs` compares COUNTS on the hand-written
//! corpus.  A count cannot see a construct found on the wrong line, or
//! one missed in one place and invented in another, and the corpus
//! holds only what someone thought to write down.  This gate parses
//! with the real `tree-sitter` CLI and compares, per construct, the
//! name and the position the server reports:
//!
//! - route definitions: kind, name, keyword position, closing-brace
//!   end, name position;
//! - `loadmodule`: bare module name and keyword position;
//! - `modparam`: module, parameter and parameter-name position;
//! - `include_file` / `import_file`: path and directive position.
//!
//! The inputs are every `.cfg` in the pinned OpenSIPS tree that
//! `opensips -C` does not reject with `syntax error`, the fixtures in
//! `tests/grammar/`, and every corpus case the grammar parses without
//! an ERROR node.

mod common;

use opensips_lsp::analyze;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One node of the tree the `tree-sitter parse` CLI prints.
#[derive(Debug)]
struct Node {
    field: Option<String>,
    kind: String,
    start: (u32, u32),
    end: (u32, u32),
    children: Vec<Node>,
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn skip_ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.b.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) {
        assert_eq!(
            self.peek(),
            Some(c),
            "tree output: expected {:?} at byte {}",
            c as char,
            self.i
        );
        self.i += 1;
    }

    /// A bare word: a node kind or a field name.
    fn word(&mut self) -> String {
        self.skip_ws();
        let s = self.i;
        while self.i < self.b.len()
            && (self.b[self.i].is_ascii_alphanumeric() || self.b[self.i] == b'_')
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.b[s..self.i]).into_owned()
    }

    /// The token a `MISSING` node names: `"}"` or a bare kind.
    fn missing_token(&mut self) -> String {
        if self.peek() == Some(b'"') {
            let s = self.i;
            self.i += 1;
            while self.i < self.b.len() && self.b[self.i] != b'"' {
                self.i += 1;
            }
            self.i += 1;
            String::from_utf8_lossy(&self.b[s..self.i]).into_owned()
        } else {
            self.word()
        }
    }

    fn number(&mut self) -> u32 {
        self.skip_ws();
        let s = self.i;
        while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
            self.i += 1;
        }
        std::str::from_utf8(&self.b[s..self.i])
            .unwrap()
            .parse()
            .unwrap_or_else(|_| panic!("tree output: expected a number at byte {s}"))
    }

    fn point(&mut self) -> (u32, u32) {
        self.expect(b'[');
        let row = self.number();
        self.expect(b',');
        let col = self.number();
        self.expect(b']');
        (row, col)
    }

    fn node(&mut self, field: Option<String>) -> Node {
        self.expect(b'(');
        let mut kind = self.word();
        if kind == "MISSING" {
            kind = format!("MISSING {}", self.missing_token());
        }
        let start = self.point();
        self.expect(b'-');
        let end = self.point();
        let mut children = Vec::new();
        loop {
            match self.peek() {
                Some(b')') => {
                    self.i += 1;
                    break;
                }
                Some(b'(') => children.push(self.node(None)),
                Some(_) => {
                    let f = self.word();
                    self.expect(b':');
                    children.push(self.node(Some(f)));
                }
                None => panic!("tree output ended inside a {kind} node"),
            }
        }
        Node {
            field,
            kind,
            start,
            end,
            children,
        }
    }
}

/// Every tree in `out`, in the order the CLI was given the files.
fn parse_trees(out: &str) -> Vec<Node> {
    let mut c = Cursor {
        b: out.as_bytes(),
        i: 0,
    };
    let mut trees = Vec::new();
    while c.peek().is_some() {
        trees.push(c.node(None));
    }
    trees
}

/// Byte offset of a (row, byte column) point in `text`.
fn offset(text: &str, (row, col): (u32, u32)) -> usize {
    let line_start: usize = text
        .split_inclusive('\n')
        .take(row as usize)
        .map(str::len)
        .sum();
    line_start + col as usize
}

fn slice<'a>(text: &'a str, n: &Node) -> &'a str {
    &text[offset(text, n.start)..offset(text, n.end)]
}

/// A string node's contents and the point its first content byte sits
/// at: the quotes stripped, the column moved past the opening one.
fn unquoted<'a>(text: &'a str, n: &Node) -> (&'a str, (u32, u32)) {
    let s = slice(text, n);
    (&s[1..s.len() - 1], (n.start.0, n.start.1 + 1))
}

fn module_base(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".so")
}

fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(n);
    for c in &n.children {
        walk(c, out);
    }
}

/// The facts the grammar's tree states about `text`, one line each,
/// in the vocabulary of [`scanner_facts`].
fn grammar_facts(text: &str, tree: &Node) -> Vec<String> {
    let mut nodes = Vec::new();
    walk(tree, &mut nodes);
    let mut out = Vec::new();
    for n in nodes {
        let strings: Vec<&Node> = n.children.iter().filter(|c| c.kind == "string").collect();
        match n.kind.as_str() {
            "route_definition" => {
                let kind = n
                    .children
                    .iter()
                    .find(|c| c.field.as_deref() == Some("kind"))
                    .map(|c| slice(text, c))
                    .unwrap_or("");
                let (name, at) = match n
                    .children
                    .iter()
                    .find(|c| c.field.as_deref() == Some("name"))
                {
                    Some(c) if c.kind == "string" => unquoted(text, c),
                    Some(c) => (slice(text, c), c.start),
                    None => ("", n.start),
                };
                out.push(format!(
                    "route {kind}[{name}] {}:{}-{}:{} name@{}:{}",
                    n.start.0, n.start.1, n.end.0, n.end.1, at.0, at.1
                ));
            }
            "loadmodule" => {
                let (path, _) = unquoted(text, strings[0]);
                out.push(format!(
                    "loadmodule {} {}:{}",
                    module_base(path),
                    n.start.0,
                    n.start.1
                ));
            }
            "modparam" => {
                let (module, _) = unquoted(text, strings[0]);
                let (param, at) = unquoted(text, strings[1]);
                out.push(format!("modparam {module}/{param} {}:{}", at.0, at.1));
            }
            "include" => {
                let (path, _) = unquoted(text, strings[0]);
                out.push(format!("include {path} {}:{}", n.start.0, n.start.1));
            }
            _ => {}
        }
    }
    out.sort();
    out
}

/// The same facts as the server's scanner finds them.
fn scanner_facts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for b in analyze::route_blocks(text) {
        out.push(format!(
            "route {}[{}] {}:{}-{}:{} name@{}:{}",
            b.kind, b.name, b.line, b.col, b.end_line, b.end_col, b.name_line, b.name_col
        ));
    }
    for m in analyze::loaded_modules(text) {
        out.push(format!("loadmodule {} {}:{}", m.name, m.line, m.col));
    }
    for m in analyze::modparam_calls(text) {
        out.push(format!(
            "modparam {}/{} {}:{}",
            m.module, m.param, m.line, m.col
        ));
    }
    for i in analyze::includes(text) {
        out.push(format!("include {} {}:{}", i.name, i.line, i.col));
    }
    out.sort();
    out
}

/// Where the two sides disagree on `text`: each fact only one side
/// states, labelled with that side.  Empty when they agree.
fn disagreements(text: &str, tree: &Node) -> Vec<String> {
    let g = grammar_facts(text, tree);
    let s = scanner_facts(text);
    let only = |a: &[String], b: &[String], who: &str| -> Vec<String> {
        let mut rest = b.to_vec();
        let mut out = Vec::new();
        for f in a {
            match rest.iter().position(|x| x == f) {
                Some(i) => {
                    rest.remove(i);
                }
                None => out.push(format!("{who} only: {f}")),
            }
        }
        out
    };
    let mut out = only(&g, &s, "grammar");
    out.extend(only(&s, &g, "scanner"));
    out
}

fn has_error(n: &Node) -> bool {
    n.kind == "ERROR" || n.kind.starts_with("MISSING") || n.children.iter().any(has_error)
}

/// Parse every file in `files` with the committed grammar, one tree
/// per file in the same order.
fn parse_with_grammar(files: &[PathBuf]) -> Vec<Node> {
    let grammar = Path::new(env!("CARGO_MANIFEST_DIR")).join("tree-sitter-opensips");
    let out = Command::new("npx")
        .args(["--yes", "tree-sitter-cli@0.26", "parse"])
        .args(files)
        .current_dir(&grammar)
        .output()
        .expect("npx tree-sitter-cli parse");
    let trees = parse_trees(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(
        trees.len(),
        files.len(),
        "one tree per file expected; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    trees
}

/// Inputs of every corpus case whose expected tree has no ERROR node:
/// past an ERROR, tree-sitter is recovering, and there is no tree of
/// the language to compare.
fn corpus_inputs() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tree-sitter-opensips/test/corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for (title, input, expected) in common::parse_corpus(&text) {
            if !expected.contains("(ERROR") {
                out.push((title, input));
            }
        }
    }
    out
}

#[test]
fn a_rigged_tree_is_reported_as_a_disagreement() {
    // The gate must SEE a divergence, or green means nothing.
    let text = "route[a] {\n    exit;\n}\n";
    let honest = "(source_file [0, 0] - [3, 0]\n  (route_definition [0, 0] - [2, 1]\n    kind: (route_kind [0, 0] - [0, 5])\n    name: (identifier [0, 6] - [0, 7])\n    body: (block [0, 9] - [2, 1]\n      (keyword_statement [1, 4] - [1, 9]))))\n";
    let trees = parse_trees(honest);
    assert_eq!(trees.len(), 1);
    assert_eq!(disagreements(text, &trees[0]), Vec::<String>::new());

    // the same route, ended one line early
    let rigged = honest.replace("[2, 1]", "[1, 9]");
    let trees = parse_trees(&rigged);
    assert_eq!(
        disagreements(text, &trees[0]),
        vec![
            "grammar only: route route[a] 0:0-1:9 name@0:6".to_string(),
            "scanner only: route route[a] 0:0-2:1 name@0:6".to_string(),
        ]
    );
}

#[test]
fn every_construct_kind_is_stated_by_both_sides() {
    // POSITIVE CONTROL: if a node kind were renamed in the grammar,
    // `grammar_facts` would silently state nothing for it and every
    // config would "agree" on zero of them.
    let text = "loadmodule \"tm.so\"\nmodparam(\"tm\", \"fr_timeout\", 5)\ninclude_file \"a.cfg\"\nroute[x] {\n    exit;\n}\n";
    let dir = common::Scratch::new("oslsp-grammar-positions-kinds");
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("kinds.cfg");
    std::fs::write(&f, text).unwrap();
    let trees = parse_with_grammar(std::slice::from_ref(&f));
    let facts = grammar_facts(text, &trees[0]);
    for kind in ["route ", "loadmodule ", "modparam ", "include "] {
        assert!(
            facts.iter().any(|f| f.starts_with(kind)),
            "the grammar stated no `{kind}` fact: {facts:?}"
        );
    }
    assert_eq!(disagreements(text, &trees[0]), Vec::<String>::new());
}

#[test]
fn grammar_and_scanner_agree_on_every_accepted_config() {
    let tree = PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let bin = common::required_env("OPENSIPS_LSP_TEST_BIN");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammar");

    let mut files = Vec::new();
    common::cfgs_under(&tree, &mut files);
    files.retain(|f| !common::syntax_error(&bin, f));
    let shipped = files.len();
    // POSITIVE CONTROL: a tree with nothing accepted makes this vacuous
    assert!(shipped >= 20, "only {shipped} shipped configs accepted");
    let mut fixture_files = Vec::new();
    common::cfgs_under(&fixtures, &mut fixture_files);
    assert!(!fixture_files.is_empty(), "the fixtures went missing");
    files.extend(fixture_files);

    let scratch = common::Scratch::new("oslsp-grammar-positions-corpus");
    std::fs::create_dir_all(&scratch).unwrap();
    let corpus = corpus_inputs();
    assert!(corpus.len() >= 8, "corpus went missing? {}", corpus.len());
    let mut titles = HashMap::new();
    for (n, (title, input)) in corpus.iter().enumerate() {
        let f = scratch.join(format!("case-{n}.cfg"));
        std::fs::write(&f, input).unwrap();
        titles.insert(f.clone(), title.clone());
        files.push(f);
    }
    files.sort();

    let trees = parse_with_grammar(&files);
    let texts: Vec<String> = files
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap())
        .collect();

    let mut report = Vec::new();
    let mut facts = 0;
    for ((f, text), tree) in files.iter().zip(&texts).zip(&trees) {
        let name = titles
            .get(f)
            .map(|t| format!("corpus case {t:?}"))
            .unwrap_or_else(|| f.display().to_string());
        assert!(
            !has_error(tree),
            "{name}: the grammar does not parse it, so there is no tree to compare"
        );
        facts += scanner_facts(text).len();
        let d = disagreements(text, tree);
        if !d.is_empty() {
            report.push(format!("{name}:\n  {}", d.join("\n  ")));
        }
    }
    assert!(
        report.is_empty(),
        "grammar and scanner disagree in {} of {} files:\n{}",
        report.len(),
        files.len(),
        report.join("\n")
    );
    // POSITIVE CONTROL: agreement on nothing is not agreement
    assert!(facts >= 100, "only {facts} facts compared");
    eprintln!("{facts} facts agree across {} files", files.len());
}
