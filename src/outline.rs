//! The structure a reader navigates a config by: the outline, folding
//! ranges and selection ranges.
//!
//! Route definitions cannot nest (`cfg.y` takes them only at the top
//! level), so the nesting in a config is the blocks inside a route
//! body: `if`, `else`, `switch`, `while`, `for`.  Those nest under
//! their route in the outline and fold on their own.  `loadmodule`,
//! `modparam` and top-level parameter assignments are listed beside
//! the route blocks rather than under one another: a `modparam` need
//! not sit next to its `loadmodule`, and an outline parent has to
//! contain its children.
//!
//! Everything here is byte-based; the server converts to UTF-16.

use crate::analyze::{self, Class, Lines};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;

/// A text extent: 0-based lines, 0-based byte columns, end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Line of the first byte.
    pub start_line: u32,
    /// Byte column of the first byte.
    pub start_col: u32,
    /// Line of the end.
    pub end_line: u32,
    /// Byte column just past the last byte.
    pub end_col: u32,
}

/// What an outline entry stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymKind {
    /// A `loadmodule` statement.
    Module,
    /// A `modparam` statement.
    Property,
    /// A top-level `name = value` parameter assignment.
    Variable,
    /// A route-family block.
    Route,
    /// A block inside a route body (`if`, `else`, `switch`, ...).
    Block,
}

/// One outline entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Sym {
    /// What the outline shows.
    pub name: String,
    /// Shown beside the name: a `modparam`'s module.
    pub detail: Option<String>,
    /// What it stands for.
    pub kind: SymKind,
    /// The whole construct.
    pub span: Span,
    /// The part an editor highlights on navigation: a name, or the
    /// keyword when there is none.
    pub sel: Span,
    /// The entries inside it.
    pub children: Vec<Sym>,
}

/// Whether a fold hides code or a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldKind {
    /// A block.
    Region,
    /// A block comment, or a run of `#` comment lines.
    Comment,
}

/// One folding range, in whole lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    /// The line that stays visible.
    pub start_line: u32,
    /// The last line hidden.
    pub end_line: u32,
    /// Block or comment.
    pub kind: FoldKind,
}

/// A block in byte offsets: its whole extent and its keyword.
struct Node {
    start: usize,
    /// Just past the closing brace.
    end: usize,
    /// Offset of the closing brace.
    close: usize,
    name: String,
    sel: Range<usize>,
    route: bool,
}

/// The route blocks and the blocks inside them, outermost first.
fn nodes(text: &str, classes: &[Class], lines: &Lines) -> Vec<Node> {
    let b = text.as_bytes();
    let mut out: Vec<Node> = Vec::new();
    let mut route_closes = HashSet::new();
    for r in analyze::route_blocks(text) {
        let start = lines.offset(r.line, r.col);
        let end = lines.offset(r.end_line, r.end_col);
        // an unterminated block runs to the end of the text
        let close = if b.get(end.wrapping_sub(1)) == Some(&b'}') {
            end - 1
        } else {
            end
        };
        route_closes.insert(close);
        let (name, sel) = if r.name.is_empty() {
            let name = if r.kind == "route" {
                "route (main)".to_string()
            } else {
                r.kind.clone()
            };
            (name, start..start + r.kind.len())
        } else {
            let ns = lines.offset(r.name_line, r.name_col);
            (format!("{}[{}]", r.kind, r.name), ns..ns + r.name.len())
        };
        out.push(Node {
            start,
            end,
            close,
            name,
            sel,
            route: true,
        });
    }
    // `cfg.y` has no braces at the top level but a route block's, so
    // every other pair is a block inside a route body
    let mut inner = Vec::new();
    for (open, close) in brace_pairs(b, classes) {
        if route_closes.contains(&close) {
            continue;
        }
        let (start, header) = header_before(b, classes, open);
        let word_end = (start..open)
            .find(|&i| !analyze::is_word_byte(b[i]))
            .unwrap_or(open);
        let sel = if word_end > start {
            start..word_end
        } else {
            open..open + 1
        };
        inner.push(Node {
            start,
            end: close + 1,
            close,
            name: if header.is_empty() {
                "{ }".to_string()
            } else {
                header
            },
            sel,
            route: false,
        });
    }
    out.extend(inner);
    out.sort_by_key(|n| (n.start, Reverse(n.end)));
    out
}

/// Matched `{`/`}` pairs in code, as (open, close) offsets.
fn brace_pairs(b: &[u8], classes: &[Class]) -> Vec<(usize, usize)> {
    let mut stack = Vec::new();
    let mut out = Vec::new();
    for (i, &c) in b.iter().enumerate() {
        if classes[i] != Class::Code {
            continue;
        }
        match c {
            b'{' => stack.push(i),
            b'}' => {
                if let Some(open) = stack.pop() {
                    out.push((open, i));
                }
            }
            _ => {}
        }
    }
    out
}

/// The statement text in front of the `{` at `open`: everything after
/// the previous `;`, `{` or `}` in code, comments dropped and runs of
/// whitespace collapsed.  Returns where it starts and the text.
fn header_before(b: &[u8], classes: &[Class], open: usize) -> (usize, String) {
    let mut from = open;
    while from > 0 {
        let i = from - 1;
        if classes[i] == Class::Code && matches!(b[i], b';' | b'{' | b'}') {
            break;
        }
        from = i;
    }
    let kept: Vec<u8> = (from..open)
        .filter(|&i| classes[i] != Class::Comment)
        .map(|i| b[i])
        .collect();
    let text = String::from_utf8_lossy(&kept)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let start = (from..open)
        .find(|&i| classes[i] != Class::Comment && !b[i].is_ascii_whitespace())
        .unwrap_or(open);
    (start, text)
}

fn span(lines: &Lines, r: Range<usize>) -> Span {
    let (start_line, start_col) = lines.line_col(r.start);
    let (end_line, end_col) = lines.line_col(r.end);
    Span {
        start_line,
        start_col,
        end_line,
        end_col,
    }
}

/// The outline: statements and route blocks in text order, with the
/// blocks of a route body nested under it.
pub fn outline(text: &str) -> Vec<Sym> {
    let classes = analyze::classify(text);
    let lines = Lines::new(text);
    let nodes = nodes(text, &classes, &lines);

    let mut top: Vec<(usize, Sym)> = Vec::new();
    for s in analyze::loadmodule_statements(text) {
        top.push((
            s.start,
            Sym {
                name: s.module.name,
                detail: None,
                kind: SymKind::Module,
                span: span(&lines, s.start..s.end),
                sel: span(&lines, s.path),
                children: Vec::new(),
            },
        ));
    }
    for s in analyze::modparam_statements(text) {
        top.push((
            s.start,
            Sym {
                name: s.call.param,
                detail: Some(s.call.module),
                kind: SymKind::Property,
                span: span(&lines, s.start..s.end),
                sel: span(&lines, s.param),
                children: Vec::new(),
            },
        ));
    }
    let in_a_route = |at: usize| nodes.iter().any(|n| n.route && n.start <= at && at < n.end);
    for (name, r, stmt) in assignments(text, &classes) {
        if in_a_route(r.start) {
            continue;
        }
        top.push((
            r.start,
            Sym {
                name,
                detail: None,
                kind: SymKind::Variable,
                span: span(&lines, stmt),
                sel: span(&lines, r),
                children: Vec::new(),
            },
        ));
    }

    // blocks, outermost first: each goes under the innermost block
    // already placed that contains it
    let mut stack: Vec<(usize, Sym)> = Vec::new();
    let place = |stack: &mut Vec<(usize, Sym)>, top: &mut Vec<(usize, Sym)>, sym: Sym| match stack
        .last_mut()
    {
        Some((_, parent)) => parent.children.push(sym),
        None => top.push((offset_of(&lines, sym.span), sym)),
    };
    for n in &nodes {
        while let Some((_, sym)) = stack.pop_if(|(end, _)| n.start >= *end) {
            place(&mut stack, &mut top, sym);
        }
        stack.push((
            n.end,
            Sym {
                name: n.name.clone(),
                detail: None,
                kind: if n.route {
                    SymKind::Route
                } else {
                    SymKind::Block
                },
                span: span(&lines, n.start..n.end),
                sel: span(&lines, n.sel.clone()),
                children: Vec::new(),
            },
        ));
    }
    while let Some((_, sym)) = stack.pop() {
        place(&mut stack, &mut top, sym);
    }
    top.sort_by_key(|(at, _)| *at);
    top.into_iter().map(|(_, s)| s).collect()
}

fn offset_of(lines: &Lines, s: Span) -> usize {
    lines.offset(s.start_line, s.start_col)
}

/// Top-level `name = value` statements: the name's extent and the
/// statement's, which ends with its line (a trailing comment and
/// whitespace left out).
fn assignments(text: &str, classes: &[Class]) -> Vec<(String, Range<usize>, Range<usize>)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let at = line_start;
        line_start += line.len();
        // blanks only: `trim_start` would also strip a blank line's
        // newline and land on the NEXT line, listing that line's
        // assignment a second time with a range ending above its start
        let lead = line.len() - line.trim_start_matches([' ', '\t', '\r']).len();
        let s = at + lead;
        if classes.get(s) != Some(&Class::Code) || !b[s].is_ascii_alphabetic() {
            continue;
        }
        let mut e = s;
        while e < b.len() && analyze::is_word_byte(b[e]) {
            e += 1;
        }
        let mut k = e;
        while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
            k += 1;
        }
        if b.get(k) != Some(&b'=') {
            continue;
        }
        let mut stmt_end = at + line.trim_end_matches(['\n', '\r']).len();
        if let Some(c) = (s..stmt_end).find(|&i| classes[i] == Class::Comment) {
            stmt_end = c;
        }
        while stmt_end > s && b[stmt_end - 1].is_ascii_whitespace() {
            stmt_end -= 1;
        }
        out.push((text[s..e].to_string(), s..e, s..stmt_end));
    }
    out
}

/// Folding ranges: every block that spans lines, every block comment
/// that does, and every run of two or more `#` comment lines.
///
/// A block whose closing brace shares its line with more code
/// (`} else {`) stops folding above that line, so the code after the
/// brace stays visible when it is folded.
pub fn folds(text: &str) -> Vec<Fold> {
    let classes = analyze::classify(text);
    let lines = Lines::new(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    for n in nodes(text, &classes, &lines) {
        let (start_line, _) = lines.line_col(n.start);
        let (close_line, _) = lines.line_col(n.close);
        let mut end_line = close_line;
        let mut i = n.close + 1;
        while i < b.len() && b[i] != b'\n' {
            if classes[i] == Class::Comment {
                break;
            }
            if !b[i].is_ascii_whitespace() {
                end_line = close_line.saturating_sub(1);
                break;
            }
            i += 1;
        }
        if end_line > start_line {
            out.push(Fold {
                start_line,
                end_line,
                kind: FoldKind::Region,
            });
        }
    }
    // comments: a block comment is one run of comment bytes that
    // starts with `/*`; a `#` comment ends at its newline, which is
    // code, so consecutive `#` lines are separate runs to be joined
    let mut i = 0;
    let mut hash_run: Option<(u32, u32)> = None;
    let flush = |run: &mut Option<(u32, u32)>, out: &mut Vec<Fold>| {
        if let Some((s, e)) = run.take()
            && e > s
        {
            out.push(Fold {
                start_line: s,
                end_line: e,
                kind: FoldKind::Comment,
            });
        }
    };
    while i < b.len() {
        if classes[i] != Class::Comment {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && classes[i] == Class::Comment {
            i += 1;
        }
        let (sl, _) = lines.line_col(start);
        let (el, _) = lines.line_col(i.saturating_sub(1));
        let alone = text[..start]
            .rsplit('\n')
            .next()
            .is_some_and(|before| before.trim().is_empty());
        if b[start] == b'#' && alone {
            match &mut hash_run {
                Some((_, e)) if *e + 1 == sl => *e = sl,
                _ => {
                    flush(&mut hash_run, &mut out);
                    hash_run = Some((sl, sl));
                }
            }
        } else {
            flush(&mut hash_run, &mut out);
            if b[start] == b'/' && el > sl {
                out.push(Fold {
                    start_line: sl,
                    end_line: el,
                    kind: FoldKind::Comment,
                });
            }
        }
    }
    flush(&mut hash_run, &mut out);
    out.sort_by_key(|f| (f.start_line, Reverse(f.end_line)));
    // an editor keeps one fold per start line: the widest
    out.dedup_by_key(|f| f.start_line);
    out
}

/// The ranges a selection grows through from byte offset `at`,
/// innermost first: the word under it, each enclosing block statement,
/// the route block, the whole document.
pub fn selection_chain(text: &str, at: usize) -> Vec<Span> {
    let classes = analyze::classify(text);
    let lines = Lines::new(text);
    let b = text.as_bytes();
    let at = at.min(b.len());
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut s = at;
    while s > 0 && analyze::is_word_byte(b[s - 1]) {
        s -= 1;
    }
    let mut e = at;
    while e < b.len() && analyze::is_word_byte(b[e]) {
        e += 1;
    }
    if e > s {
        ranges.push(s..e);
    }
    let mut around: Vec<Range<usize>> = nodes(text, &classes, &lines)
        .into_iter()
        .filter(|n| n.start <= at && at < n.end)
        .map(|n| n.start..n.end)
        .collect();
    around.sort_by_key(|r| r.end - r.start);
    ranges.extend(around);
    ranges.push(0..b.len());
    ranges.dedup();
    ranges.into_iter().map(|r| span(&lines, r)).collect()
}
