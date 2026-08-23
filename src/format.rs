//! Line-preserving whitespace formatter for `opensips.cfg`.
//!
//! The formatter rewrites the leading and trailing whitespace of a
//! line and nothing else: it never joins, splits or reorders lines,
//! never touches a byte inside a string literal or a comment body, and
//! never rewrites a line it does not need to change.
//!
//! That restraint is the design.  A config is not source code that can
//! be re-parsed and pretty-printed at leisure — it is a live routing
//! script, and a formatter that reflowed it into a different parse
//! would be worse than no formatter at all.  Indentation is the part
//! that is provably safe to normalise, and it is also the part that
//! actually rots in a config edited by many hands.
//!
//! Three things are deliberately left alone:
//!
//! * **Continuation lines of a multi-line string or block comment.**
//!   Their leading whitespace is content, not layout.
//! * **`#!` preprocessor directives.** OpenSIPS processes them
//!   line-wise ahead of the parser; their column is not ours to move.
//! * **Lines that continue the previous statement.**  Brace depth is
//!   not the whole story about indentation here.  A call whose
//!   arguments span lines, a condition broken across lines, and the
//!   body of a braceless `if` are all indented by the author to show
//!   what they belong to, and none of that shows up in the brace
//!   count.  Re-indenting them to brace depth flattens deliberate
//!   layout and — for a braceless `if` — makes the body read as
//!   though it runs unconditionally.  So a line is only re-indented
//!   when the previous code line actually ENDED a statement.

use crate::analyze::{Class, classify};

/// Formatting options, as an LSP client supplies them in
/// `FormattingOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Indent with spaces rather than tabs.
    pub insert_spaces: bool,
    /// Spaces per indent level (ignored when indenting with tabs).
    pub tab_size: u32,
}

impl Default for Options {
    /// Tabs, matching the upstream `etc/opensips.cfg` convention, for
    /// clients that send no options at all.
    fn default() -> Self {
        Self {
            insert_spaces: false,
            tab_size: 4,
        }
    }
}

/// One line the formatter would rewrite, with its full new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    /// 0-based line number.
    pub line: u32,
    /// The line's new text, without its line ending.
    pub text: String,
}

impl Options {
    /// The literal indent string for `depth` levels.
    fn indent(&self, depth: usize) -> String {
        if self.insert_spaces {
            " ".repeat(self.tab_size.max(1) as usize * depth)
        } else {
            "\t".repeat(depth)
        }
    }
}

/// A line's raw text plus the byte range it occupies in the document.
struct Line<'a> {
    text: &'a str,
    start: usize,
}

/// Split into lines, keeping each line's byte offset.  Unlike
/// [`str::lines`] this reports a trailing empty line for a document
/// that ends with a newline, so offsets stay exact.
fn lines_with_offsets(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            out.push(Line {
                text: &text[start..i],
                start,
            });
            start = i + 1;
        }
    }
    out.push(Line {
        text: &text[start..],
        start,
    });
    out
}

/// Did this line end a statement?  Only `;`, `{` and `}` in code
/// position do; anything else means the next line continues it.
fn ends_statement(body: &str, start: usize, class: &[Class]) -> Option<bool> {
    let mut last = None;
    for (off, ch) in body.char_indices() {
        if ch.is_whitespace() || class.get(start + off) != Some(&Class::Code) {
            continue;
        }
        last = Some(ch);
    }
    last.map(|c| matches!(c, ';' | '{' | '}'))
}

/// The rewritten text of every line, in order.  Lines the formatter
/// leaves alone come back byte-identical.
fn formatted_lines(text: &str, opts: &Options) -> Vec<String> {
    let class = classify(text);
    let lines = lines_with_offsets(text);
    let mut out = Vec::with_capacity(lines.len());
    let mut depth: usize = 0;
    // whether the last line carrying code closed its statement; blank
    // and comment-only lines do not change the answer
    let mut prev_ended = true;

    for line in &lines {
        // A line continues a multi-line string or block comment when
        // the newline that ended the previous line is itself inside
        // one.  A `#` comment never reaches its own newline, so this
        // distinguishes the two cases exactly.
        let continues_literal = line
            .start
            .checked_sub(1)
            .and_then(|i| class.get(i))
            .is_some_and(|c| matches!(c, Class::Str | Class::Comment));

        // Carriage returns belong to the line ending, not the content.
        let (body, cr) = match line.text.strip_suffix('\r') {
            Some(b) => (b, "\r"),
            None => (line.text, ""),
        };

        if continues_literal {
            out.push(line.text.to_string());
            // depth still has to track code braces, but there are
            // none inside a literal
            continue;
        }

        let trimmed = body.trim();
        if trimmed.is_empty() {
            out.push(String::new() + cr);
            continue;
        }

        // Braces that are code decide the depth; a `{` in a string or
        // a comment is decoration.
        let leading_ws = body.len() - body.trim_start().len();
        let code_at = |off: usize| class.get(line.start + off) == Some(&Class::Code);

        // Closers at the head of the line dedent the line itself.
        let mut closers = 0usize;
        for (off, ch) in body.char_indices().skip_while(|(i, _)| *i < leading_ws) {
            if ch == '}' && code_at(off) {
                closers += 1;
            } else if !ch.is_whitespace() {
                break;
            }
        }

        let indent = if trimmed.starts_with("#!") || !prev_ended {
            // not ours to move: a preprocessor directive is read
            // line-wise ahead of the parser, and a line continuing the
            // previous statement is indented by its author to show
            // what it belongs to
            body[..leading_ws].to_string()
        } else {
            opts.indent(depth.saturating_sub(closers))
        };
        out.push(format!("{indent}{trimmed}{cr}"));
        if let Some(ended) = ends_statement(body, line.start, &class) {
            prev_ended = ended;
        }

        // net brace delta over the code positions of this line
        for (off, ch) in body.char_indices() {
            if !code_at(off) {
                continue;
            }
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    out
}

/// The whole document, reformatted.
pub fn format(text: &str, opts: &Options) -> String {
    formatted_lines(text, opts).join("\n")
}

/// Only the lines the formatter would change.  Rewriting solely what
/// moved keeps the client's folding, selection and cursor intact,
/// which a whole-document replacement destroys.
pub fn format_lines(text: &str, opts: &Options) -> Vec<LineEdit> {
    let originals = lines_with_offsets(text);
    formatted_lines(text, opts)
        .into_iter()
        .enumerate()
        .filter(|(i, new)| originals[*i].text != new)
        .map(|(i, text)| LineEdit {
            line: i as u32,
            text,
        })
        .collect()
}

/// The lines the formatter would change within `[start_line,
/// end_line]` inclusive.  The depth is still computed from the whole
/// document, so a formatted range lands at the indentation it would
/// have had in a full-document pass.
pub fn format_range(text: &str, opts: &Options, start_line: u32, end_line: u32) -> Vec<LineEdit> {
    format_lines(text, opts)
        .into_iter()
        .filter(|e| e.line >= start_line && e.line <= end_line)
        .collect()
}
