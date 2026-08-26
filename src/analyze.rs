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
pub(crate) enum Class {
    Code,
    Str,
    Comment,
}

/// Per-byte classification: code, string interior, or comment.
pub(crate) fn classify(text: &str) -> Vec<Class> {
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
            // OpenSIPS strings come double- OR single-quoted
            q @ (b'"' | b'\'') => {
                // opening quote counts as code; interior as string
                i += 1;
                while i < b.len() && b[i] != q {
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

/// Word byte, for callers outside this module.
pub fn is_word_byte(c: u8) -> bool {
    is_word(c)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

macro_rules! static_regex {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static regex::Regex {
            static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

static_regex!(re_loadmodule, r#"loadmodule\s*"([^"\n]+)""#);
static_regex!(
    re_route_def,
    r#"(?s)(failure_route|onreply_route|branch_route|timer_route|event_route|error_route|local_route|startup_route|route)\s*(?:\[\s*["']?([A-Za-z0-9_.:-]+)["']?\s*(?:,\s*\d+\s*)?\])?\s*\{"#
);
static_regex!(
    re_route_ref,
    r#"route\s*\(\s*["']?([A-Za-z0-9_.:-]+)["']?\s*[,)]"#
);
static_regex!(
    re_modparam_ctx,
    r#"modparam\s*\(\s*"([^"\n]+)"\s*,\s*("[^"\n]*)?$"#
);

/// A function call found in code position, with where each of its
/// top-level arguments begins.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The called name as written.
    pub name: String,
    /// 0-based line of the name.
    pub line: u32,
    /// 0-based start column of the name.
    pub col: u32,
    /// 0-based (line, col) of the first real character of each
    /// top-level argument.  Empty for a call with no arguments.
    pub args: Vec<(u32, u32)>,
}

/// Every `name(...)` call in code position, with its argument
/// positions.
///
/// This is a scan, not a parse: it exists so inlay hints know where an
/// argument starts.  Commas inside strings or nested parentheses do
/// not split arguments, and a call written inside a string or a
/// comment is not a call.  Keywords like `if` and `route` come back
/// too — the catalogue lookup that consumes this simply will not know
/// them.
pub fn calls(text: &str) -> Vec<Call> {
    let class = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !is_word(b[i]) || class[i] != Class::Code {
            i += 1;
            continue;
        }
        // a name is only a name when nothing wordy precedes it
        if i > 0 && is_word(b[i - 1]) {
            while i < b.len() && is_word(b[i]) {
                i += 1;
            }
            continue;
        }
        let name_start = i;
        while i < b.len() && is_word(b[i]) {
            i += 1;
        }
        let name_end = i;
        let mut j = i;
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        if j >= b.len() || b[j] != b'(' || class[j] != Class::Code {
            continue;
        }
        // walk the argument list, tracking nesting and quotes so a
        // comma inside either is not a separator
        let mut depth = 0i32;
        let mut quote: Option<u8> = None;
        let mut seg_start = j + 1;
        let mut args: Vec<(u32, u32)> = Vec::new();
        let mut k = j;
        let mut closed = false;
        while k < b.len() {
            let c = b[k];
            match quote {
                Some(q) => {
                    if c == b'\\' {
                        k += 2;
                        continue;
                    }
                    if c == q {
                        quote = None;
                    }
                }
                None => match c {
                    b'"' | b'\'' => quote = Some(c),
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            push_arg(text, seg_start, k, &mut args);
                            closed = true;
                            break;
                        }
                    }
                    b',' if depth == 1 => {
                        push_arg(text, seg_start, k, &mut args);
                        seg_start = k + 1;
                    }
                    _ => {}
                },
            }
            k += 1;
        }
        let (line, col) = line_col(text, name_start);
        out.push(Call {
            name: text[name_start..name_end].to_string(),
            line,
            col,
            args: if closed { args } else { Vec::new() },
        });
        i = name_end;
    }
    out
}

/// Record `text[from..to]` as an argument, unless it is blank.
fn push_arg(text: &str, from: usize, to: usize, out: &mut Vec<(u32, u32)>) {
    let seg = &text[from..to.min(text.len())];
    let Some(off) = seg.find(|c: char| !c.is_whitespace()) else {
        return;
    };
    out.push(line_col(text, from + off));
}

/// Every `loadmodule "x.so"` in code position, as bare module names.
pub fn loaded_modules(text: &str) -> Vec<Located> {
    let classes = classify(text);
    let mut out = Vec::new();
    for c in re_loadmodule().captures_iter(text) {
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

/// Every route-family block definition (`route`, `failure_route[x]`, ...);
/// the main `route { }` has an empty name.
pub fn route_defs(text: &str) -> Vec<Located> {
    route_blocks(text)
        .into_iter()
        .map(|b| Located {
            name: b.name,
            line: b.line,
            col: b.col,
        })
        .collect()
}

/// A route-family block definition with its full brace extent.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Route name; empty for the main `route { }`.
    pub name: String,
    /// Block keyword (`route`, `failure_route`, ...).
    pub kind: String,
    /// 0-based line of the keyword.
    pub line: u32,
    /// 0-based start column of the keyword.
    pub col: u32,
    /// 0-based line of the closing brace (last line if unterminated).
    pub end_line: u32,
    /// 0-based column just past the closing brace.
    pub end_col: u32,
    /// 0-based line of the route NAME (keyword line if unnamed).
    pub name_line: u32,
    /// 0-based start column of the route NAME (keyword col if unnamed).
    pub name_col: u32,
}

/// [`route_defs`] with block extents: braces are matched through the
/// comment/string classifier, so a `}` in a string or comment does not
/// close a block; an unterminated block extends to end of text.
pub fn route_blocks(text: &str) -> Vec<Block> {
    let classes = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    for c in re_route_def().captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        if classes.get(start) != Some(&Class::Code) {
            continue;
        }
        // reject matches that are a tail of a longer identifier
        if start > 0 && is_word(b[start - 1]) {
            continue;
        }
        // the match ends just past the opening brace
        let mut depth = 1usize;
        let mut i = whole.end();
        let mut close = None;
        while i < b.len() {
            if classes[i] == Class::Code {
                match b[i] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        let (line, col) = line_col(text, start);
        let (end_line, end_col) = match close {
            Some(p) => {
                let (l, c2) = line_col(text, p);
                (l, c2 + 1)
            }
            None => line_col(text, text.len()),
        };
        let (name_line, name_col) = c
            .get(2)
            .map(|m| line_col(text, m.start()))
            .unwrap_or((line, col));
        out.push(Block {
            name: c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            kind: c.get(1).unwrap().as_str().to_string(),
            line,
            col,
            end_line,
            end_col,
            name_line,
            name_col,
        });
    }
    out
}

/// Every `route(name)` call site (excluding `*_route(...)` look-alikes).
pub fn route_refs(text: &str) -> Vec<Located> {
    let classes = classify(text);
    let re = re_route_ref();
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

static_regex!(
    re_include,
    r#"(?:include_file|import_file)\s*(?:"([^"\n]+)"|'([^'\n]+)')"#
);

/// Every `include_file "x"` / `import_file "x"` in code position;
/// `name` is the quoted path verbatim.
pub fn includes(text: &str) -> Vec<Located> {
    let mut out = Vec::new();
    for c in re_include().captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        // The flattener is line-oriented and runs BEFORE anything is
        // lexed: `cfg_pp.c:mk_included_file_path` skips leading
        // whitespace and matches the directive there, knowing nothing
        // about comments or strings. So it fires inside a `/* */`
        // block, and does NOT fire when anything else opens the line
        // — including a tail like `reinclude_file`.
        //
        // Reading this as code position is the LEXER's rule, and it
        // disagrees with the parser in both directions: a block
        // commented out with `/* */` still loads its includes, and a
        // directive after a statement on the same line does not.
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        if !text[line_start..start].trim().is_empty() {
            continue;
        }
        let path = c
            .get(1)
            .or_else(|| c.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        if path.is_empty() || path.contains('\0') {
            continue;
        }
        let (line, col) = line_col(text, start);
        out.push(Located {
            name: path.to_string(),
            line,
            col,
        });
    }
    out
}

/// One `modparam("module", "param", ...)` call site.
#[derive(Debug, Clone, PartialEq)]
pub struct ModparamCall {
    /// First argument: the module name.
    pub module: String,
    /// Second argument: the parameter name.
    pub param: String,
    /// 0-based line of the PARAM name.
    pub line: u32,
    /// 0-based start column of the PARAM name (inside its quotes).
    pub col: u32,
}

static_regex!(
    re_modparam_call,
    r#"modparam\s*\(\s*["']([^"'\n]+)["']\s*,\s*["']([^"'\n]+)["']"#
);

/// Every `modparam("m", "p", ...)` in code position, with the
/// position of the parameter name.
pub fn modparam_calls(text: &str) -> Vec<ModparamCall> {
    let classes = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    for c in re_modparam_call().captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        if classes.get(start) != Some(&Class::Code) {
            continue;
        }
        if start > 0 && is_word(b[start - 1]) {
            continue;
        }
        let (module, param) = (&c[1], &c[2]);
        if module.contains('\0') || param.contains('\0') {
            continue;
        }
        let pm = c.get(2).unwrap();
        let (line, col) = line_col(text, pm.start());
        out.push(ModparamCall {
            module: module.to_string(),
            param: param.to_string(),
            line,
            col,
        });
    }
    out
}

static_regex!(
    re_pvar_tok,
    r"\$[A-Za-z][A-Za-z0-9_.]*(?:\([A-Za-z0-9_.:>=-]*\))?"
);

/// Every pseudo-variable occurrence as (line, col, byte length).
/// Pvars inside strings COUNT — OpenSIPS interpolates them there;
/// comments (line or block) never contribute.
pub fn pvars(text: &str) -> Vec<(u32, u32, u32)> {
    let classes = classify(text);
    let mut out = Vec::new();
    for m in re_pvar_tok().find_iter(text) {
        match classes.get(m.start()) {
            Some(&Class::Code) | Some(&Class::Str) => {}
            _ => continue,
        }
        let (line, col) = line_col(text, m.start());
        out.push((line, col, (m.end() - m.start()) as u32));
    }
    out
}

/// If the cursor (end of `line_prefix`) sits inside the *second*
/// string argument of a `modparam(...)`, return the module name from
/// the first argument.
pub fn modparam_context(line_prefix: &str) -> Option<String> {
    let re = re_modparam_ctx();
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
