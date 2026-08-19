//! Lightweight lexical analysis of opensips.cfg documents.
//!
//! Not a grammar: a comment/string-aware scanner good enough for
//! completion, symbols and go-to-definition.  Full-fidelity semantic
//! validation is delegated to `opensips -C` (see `diag`).

/// A named item found in a cfg document, with its position.
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    /// Item name (module base name, route name, ...).
    pub name: String,
    /// 0-based line of the item's name.
    pub line: u32,
    /// 0-based start column of the name.
    pub col: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Code,
    Str,
    Comment,
}

/// Per-byte classification: code, string interior, or comment.
fn classify(text: &str) -> Vec<Class> {
    let b = text.as_bytes();
    let mut out = vec![Class::Code; b.len()];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = Class::Comment;
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                out[i] = Class::Comment;
                out[i + 1] = Class::Comment;
                i += 2;
                while i < b.len() && !(b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/') {
                    out[i] = Class::Comment;
                    i += 1;
                }
                if i + 1 < b.len() {
                    out[i] = Class::Comment;
                    out[i + 1] = Class::Comment;
                    i += 2;
                }
            }
            b'"' => {
                // opening quote counts as code; interior as string
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    out[i] = Class::Str;
                    if b[i] == b'\\' && i + 1 < b.len() {
                        out[i + 1] = Class::Str;
                        i += 1;
                    }
                    i += 1;
                }
                if i < b.len() {
                    i += 1; // closing quote
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// Byte offset → (0-based line, 0-based col).
fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let pre = &text.as_bytes()[..offset.min(text.len())];
    let line = pre.iter().filter(|&&c| c == b'\n').count() as u32;
    let col = offset
        - pre
            .iter()
            .rposition(|&c| c == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
    (line, col as u32)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Every `loadmodule "x.so"` in code position, as bare module names.
pub fn loaded_modules(text: &str) -> Vec<Located> {
    let classes = classify(text);
    let re = regex::Regex::new(r#"loadmodule\s*"([^"\n]+)""#).unwrap();
    let mut out = Vec::new();
    for c in re.captures_iter(text) {
        let whole = c.get(0).unwrap();
        if classes.get(whole.start()) != Some(&Class::Code) {
            continue; // inside a comment or a string
        }
        let path = c.get(1).unwrap();
        let base = path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".so");
        if base.is_empty() || base.contains('\0') {
            continue;
        }
        let (line, col) = line_col(text, whole.start());
        out.push(Located {
            name: base.to_string(),
            line,
            col,
        });
    }
    out
}

const ROUTE_KINDS: &str = r"(?:failure_route|onreply_route|branch_route|timer_route|event_route|error_route|local_route|startup_route|route)";

/// Every route-family block definition (`route`, `failure_route[x]`, ...);
/// the main `route { }` has an empty name.
pub fn route_defs(text: &str) -> Vec<Located> {
    let classes = classify(text);
    let re = regex::Regex::new(&format!(
        r#"(?s){ROUTE_KINDS}\s*(?:\[\s*"?([A-Za-z0-9_.:-]+)"?\s*\])?\s*\{{"#
    ))
    .unwrap();
    let mut out = Vec::new();
    for c in re.captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        if classes.get(start) != Some(&Class::Code) {
            continue;
        }
        // reject matches that are a tail of a longer identifier
        if start > 0 && is_word(text.as_bytes()[start - 1]) {
            continue;
        }
        let name = c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let (line, col) = line_col(text, start);
        out.push(Located { name, line, col });
    }
    out
}

/// Every `route(name)` call site (excluding `*_route(...)` look-alikes).
pub fn route_refs(text: &str) -> Vec<Located> {
    let classes = classify(text);
    let re = regex::Regex::new(r#"route\s*\(\s*"?([A-Za-z0-9_.:-]+)"?\s*[,)]"#).unwrap();
    let mut out = Vec::new();
    for c in re.captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        if classes.get(start) != Some(&Class::Code) {
            continue;
        }
        // exclude failure_route(...), onreply_route(...) etc.
        if start > 0 && is_word(text.as_bytes()[start - 1]) {
            continue;
        }
        let name = c.get(1).unwrap();
        let (line, col) = line_col(text, name.start());
        out.push(Located {
            name: name.as_str().to_string(),
            line,
            col,
        });
    }
    out
}

/// If the cursor (end of `line_prefix`) sits inside the *second*
/// string argument of a `modparam(...)`, return the module name from
/// the first argument.
pub fn modparam_context(line_prefix: &str) -> Option<String> {
    let re = regex::Regex::new(r#"modparam\s*\(\s*"([^"\n]+)"\s*,\s*("[^"\n]*)?$"#).unwrap();
    re.captures(line_prefix).map(|c| c[1].to_string())
}

/// The `[A-Za-z0-9_]+` identifier covering byte column `col`, if any.
pub fn word_at(line: &str, col: usize) -> Option<String> {
    let b = line.as_bytes();
    if col >= b.len() || !is_word(b[col]) {
        return None;
    }
    let mut s = col;
    while s > 0 && is_word(b[s - 1]) {
        s -= 1;
    }
    let mut e = col;
    while e < b.len() && is_word(b[e]) {
        e += 1;
    }
    std::str::from_utf8(&b[s..e]).ok().map(|w| w.to_string())
}

/// Convert an LSP UTF-16 code-unit column to a byte offset in `line`,
/// clamping past-end positions to the line length and positions
/// inside a surrogate pair to the character's start.
pub fn utf16_to_byte(line: &str, utf16_col: u32) -> usize {
    let mut units = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if units >= utf16_col {
            return byte_idx;
        }
        let w = ch.len_utf16() as u32;
        if units + w > utf16_col {
            // inside a surrogate pair: clamp to the char start
            return byte_idx;
        }
        units += w;
    }
    line.len()
}

/// Convert a byte offset in `line` to an LSP UTF-16 code-unit column,
/// clamping past-end offsets to the line's length in units.
pub fn byte_to_utf16(line: &str, byte_col: usize) -> u32 {
    let mut units = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if byte_idx >= byte_col {
            return units;
        }
        units += ch.len_utf16() as u32;
    }
    units
}
