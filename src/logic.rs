//! Editor-feature logic, pure and testable.

use crate::analyze::{self, Located};
use crate::catalog::ModuleDoc;

/// What a completion item is, mapped to an LSP `CompletionItemKind`.
#[derive(Debug, Clone, PartialEq)]
pub enum CompKind {
    /// A loadable module name.
    Module,
    /// A `modparam` parameter of a module.
    Param,
    /// An exported script function.
    Function,
    /// A `route[...]` name.
    Route,
    /// A core language keyword.
    Keyword,
}

/// One completion candidate.
#[derive(Debug, Clone)]
pub struct Comp {
    /// The inserted/displayed label.
    pub label: String,
    /// Short detail (type, signature, or category).
    pub detail: String,
    /// Markdown documentation, empty if none.
    pub doc: String,
    /// Item category.
    pub kind: CompKind,
}

/// A small core-keyword seed; module functions dominate real usage.
const CORE_KEYWORDS: &[&str] = &[
    "if",
    "else",
    "switch",
    "case",
    "default",
    "while",
    "for",
    "exit",
    "drop",
    "return",
    "break",
    "route",
    "xlog",
    "forward",
    "setflag",
    "resetflag",
    "isflagset",
    "async",
    "launch",
];

macro_rules! static_regex {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static regex::Regex {
            static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

static_regex!(re_modparam_first_arg, r#"modparam\s*\(\s*"[^"]*$"#);
static_regex!(re_loadmodule_arg, r#"loadmodule\s*"[^"]*$"#);
static_regex!(
    re_route_call_arg,
    r#"(?:^|[^A-Za-z0-9_])route\s*\(\s*"?[A-Za-z0-9_.:-]*$"#
);

/// Is the cursor inside the name argument of a `route(...)` call?
fn route_call_context(line_prefix: &str) -> bool {
    re_route_call_arg().is_match(line_prefix)
}

/// Context-sensitive completion candidates for a cursor whose line
/// starts with `line_prefix`: modparam values/modules, loadmodule
/// targets, or (in code) loaded modules' functions, routes, keywords.
pub fn completions(catalog: &[ModuleDoc], doc: &str, line_prefix: &str) -> Vec<Comp> {
    let files = [(std::path::PathBuf::new(), doc.to_string())];
    complete_files(
        catalog,
        &crate::catalog::CoreDocs::default(),
        &files,
        line_prefix,
    )
}

/// The completion engine over an include closure (`files`: the
/// current document first, included files after).
fn complete_files(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    files: &[(std::path::PathBuf, String)],
    line_prefix: &str,
) -> Vec<Comp> {
    // "$" prefix → pseudo-variables, nothing else
    if pvar_tail(line_prefix).is_some() {
        return core
            .pvars
            .iter()
            .map(|v| Comp {
                label: v.name.clone(),
                detail: v.detail.clone(),
                doc: v.doc.clone(),
                kind: CompKind::Keyword,
            })
            .collect();
    }
    // modparam second argument → params of the named module
    if let Some(module) = analyze::modparam_context(line_prefix) {
        return catalog
            .iter()
            .filter(|m| m.name == module)
            .flat_map(|m| m.params.iter())
            .map(|p| Comp {
                label: p.name.clone(),
                detail: p.detail.clone(),
                doc: p.doc.clone(),
                kind: CompKind::Param,
            })
            .collect();
    }
    // modparam first argument → module names
    if re_modparam_first_arg().is_match(line_prefix) {
        return catalog
            .iter()
            .map(|m| Comp {
                label: m.name.clone(),
                detail: "module".into(),
                doc: String::new(),
                kind: CompKind::Module,
            })
            .collect();
    }
    // loadmodule argument → module .so names
    if re_loadmodule_arg().is_match(line_prefix) {
        return catalog
            .iter()
            .map(|m| Comp {
                label: format!("{}.so", m.name),
                detail: "module".into(),
                doc: String::new(),
                kind: CompKind::Module,
            })
            .collect();
    }

    let route_comp = |name: String| Comp {
        label: name,
        detail: "route".into(),
        doc: String::new(),
        kind: CompKind::Route,
    };
    let route_names = || {
        route_defs_multi(files)
            .into_iter()
            .filter(|(_, r)| !r.name.is_empty())
            .map(|(_, r)| r.name)
    };
    // route(<cursor>) → route names only
    if route_call_context(line_prefix) {
        return route_names().map(route_comp).collect();
    }

    // plain code: functions of LOADED modules (anywhere in the
    // closure) + route names + keywords + core items
    let loaded = loaded_modules_multi(files);
    let mut out: Vec<Comp> = catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .flat_map(|m| m.functions.iter())
        .map(|f| Comp {
            label: f.name.clone(),
            detail: f.detail.clone(),
            doc: f.doc.clone(),
            kind: CompKind::Function,
        })
        .collect();
    out.extend(route_names().map(route_comp));
    for k in CORE_KEYWORDS {
        out.push(Comp {
            label: (*k).into(),
            detail: "keyword".into(),
            doc: String::new(),
            kind: CompKind::Keyword,
        });
    }
    for f in &core.functions {
        out.push(Comp {
            label: f.name.clone(),
            detail: f.detail.clone(),
            doc: f.doc.clone(),
            kind: CompKind::Function,
        });
    }
    for p in &core.params {
        out.push(Comp {
            label: p.name.clone(),
            detail: p.detail.clone(),
            doc: p.doc.clone(),
            kind: CompKind::Param,
        });
    }
    dedup_completions(out)
}

/// [`completions_with_core`] over an include closure.
pub fn completions_with_core_files(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    files: &[(std::path::PathBuf, String)],
    line_prefix: &str,
) -> Vec<Comp> {
    complete_files(catalog, core, files, line_prefix)
}

/// Markdown hover for `word`: loaded-module symbols win, then any
/// module's symbols, then module names themselves.
pub fn hover_markdown(catalog: &[ModuleDoc], doc: &str, word: &str) -> Option<String> {
    let loaded: Vec<String> = analyze::loaded_modules(doc)
        .into_iter()
        .map(|m| m.name)
        .collect();
    let ordered = catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .chain(catalog.iter().filter(|m| !loaded.contains(&m.name)));

    for m in ordered {
        if let Some(f) = m.functions.iter().find(|f| f.name == word) {
            return Some(format!(
                "```\n{}\n```\n*module {}*\n\n{}",
                f.detail, m.name, f.doc
            ));
        }
        if let Some(p) = m.params.iter().find(|p| p.name == word) {
            return Some(format!(
                "**{}** ({}) — modparam of `{}`\n\n{}",
                p.name, p.detail, m.name, p.doc
            ));
        }
    }
    catalog.iter().find(|m| m.name == word).map(|m| {
        format!(
            "**module {}** — {} params, {} functions",
            m.name,
            m.params.len(),
            m.functions.len()
        )
    })
}

/// Is byte position (line, col) inside the name span starting at `l`?
fn in_span(l: &Located, name: &str, line: u32, col: u32) -> bool {
    l.line == line && col >= l.col && col < l.col + name.len() as u32
}

/// Go-to-definition: a route name under the cursor resolves to its
/// `route[name]` block.  Matching is span-based, so dotted names like
/// `to.b` resolve whole (they are legal in route names).
pub fn definition_of(doc: &str, line: u32, col: u32) -> Option<Located> {
    let name = analyze::route_refs(doc)
        .into_iter()
        .find(|r| in_span(r, &r.name, line, col))?
        .name;
    analyze::route_defs(doc)
        .into_iter()
        .find(|d| d.name == name)
}

/// The route name whose span covers byte position (line, col) — at a
/// `route(name)` call site or at the NAME inside a `route[name]`
/// definition.  The anonymous main route has no symbol.
pub fn route_symbol_at(doc: &str, line: u32, col: u32) -> Option<String> {
    if let Some(r) = analyze::route_refs(doc)
        .into_iter()
        .find(|r| in_span(r, &r.name, line, col))
    {
        return Some(r.name);
    }
    analyze::route_blocks(doc)
        .into_iter()
        .filter(|b| !b.name.is_empty())
        .find(|b| {
            let at = Located {
                name: b.name.clone(),
                line: b.name_line,
                col: b.name_col,
            };
            in_span(&at, &b.name, line, col)
        })
        .map(|b| b.name)
}

/// Every occurrence of route `name` in `doc`: call sites and the
/// definition's name span.  The bool is `true` for the definition.
pub fn route_occurrences(doc: &str, name: &str) -> Vec<(Located, bool)> {
    if name.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(Located, bool)> = analyze::route_refs(doc)
        .into_iter()
        .filter(|r| r.name == name)
        .map(|r| (r, false))
        .collect();
    for b in analyze::route_blocks(doc) {
        if b.name == name {
            out.push((
                Located {
                    name: b.name,
                    line: b.name_line,
                    col: b.name_col,
                },
                true,
            ));
        }
    }
    out
}

/// Is `s` legal as an UNQUOTED route name?  Gates rename, which
/// splices the new name into unquoted positions: the cfg lexer only
/// accepts `ID = [A-Za-z][A-Za-z0-9_]*` or a NUMBER there (dotted,
/// dashed, and colon names exist but must be quoted).
pub fn valid_route_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let b = s.as_bytes();
    if b.iter().all(|c| c.is_ascii_digit()) {
        return true;
    }
    b[0].is_ascii_alphabetic()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Transitive include limits: a hostile config must stay cheap.
const INCLUDE_MAX_DEPTH: usize = 8;
const INCLUDE_MAX_FILES: usize = 64;

/// Resolve an `include_file`/`import_file` path: absolute paths as
/// written, relative paths against the INCLUDING file's directory.
fn resolve_include(from: &std::path::Path, inc: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(inc);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        from.parent()
            .unwrap_or_else(|| std::path::Path::new(""))
            .join(p)
    }
}

/// The root document plus every transitively included file, loaded via
/// `loader` (which lets callers prefer open editor buffers over disk).
/// Cycle-safe, depth- and count-capped; unloadable files are skipped.
pub fn include_closure(
    root_path: &std::path::Path,
    root_text: &str,
    loader: &dyn Fn(&std::path::Path) -> Option<String>,
) -> Vec<(std::path::PathBuf, String)> {
    let mut out: Vec<(std::path::PathBuf, String)> = Vec::new();
    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    let mut queue: Vec<(std::path::PathBuf, String, usize)> =
        vec![(root_path.to_path_buf(), root_text.to_string(), 0)];
    seen.insert(root_path.to_path_buf());
    while let Some((path, text, depth)) = queue.pop() {
        if depth < INCLUDE_MAX_DEPTH {
            for inc in analyze::includes(&text) {
                if out.len() + queue.len() + 1 >= INCLUDE_MAX_FILES {
                    break;
                }
                let target = resolve_include(&path, &inc.name);
                if !seen.insert(target.clone()) {
                    continue;
                }
                if let Some(t) = loader(&target) {
                    queue.push((target, t, depth + 1));
                }
            }
        }
        out.push((path, text));
    }
    // root first, includes after, in a stable order
    out.sort_by(|a, b| {
        (a.0 != *root_path)
            .cmp(&(b.0 != *root_path))
            .then(a.0.cmp(&b.0))
    });
    out
}

/// Module names loaded anywhere in the closure, deduplicated.
pub fn loaded_modules_multi(files: &[(std::path::PathBuf, String)]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (_, text) in files {
        for m in analyze::loaded_modules(text) {
            if seen.insert(m.name.clone()) {
                out.push(m.name);
            }
        }
    }
    out
}

/// Route definitions across the closure, tagged with their file.
pub fn route_defs_multi(
    files: &[(std::path::PathBuf, String)],
) -> Vec<(std::path::PathBuf, Located)> {
    files
        .iter()
        .flat_map(|(p, text)| {
            analyze::route_defs(text)
                .into_iter()
                .map(move |l| (p.clone(), l))
        })
        .collect()
}

/// A fast analyzer-side diagnostic (no `opensips -C` involved).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzerDiag {
    /// 0-based line in the CURRENT file.
    pub line: u32,
    /// 0-based start byte column.
    pub col_start: u32,
    /// 0-based end byte column (exclusive).
    pub col_end: u32,
    /// Human-readable message.
    pub message: String,
}

/// Analyzer diagnostics for `text` (the file at `path`): `route(x)`
/// calls whose target is defined nowhere in the include closure, and
/// duplicate route definitions (every occurrence after the first is
/// flagged).  Only positions in the current file are reported.
pub fn analyzer_diagnostics(
    path: &std::path::Path,
    text: &str,
    loader: &dyn Fn(&std::path::Path) -> Option<String>,
) -> Vec<AnalyzerDiag> {
    let files = include_closure(path, text, loader);
    let blocks: Vec<(std::path::PathBuf, analyze::Block)> = files
        .iter()
        .flat_map(|(p, t)| {
            analyze::route_blocks(t)
                .into_iter()
                .map(move |b| (p.clone(), b))
        })
        .collect();
    // `route(x)` targets REQUEST routes only: failure_route[x] etc.
    // live in separate namespaces and do not satisfy the call
    let defined: std::collections::HashSet<&str> = blocks
        .iter()
        .filter(|(_, b)| b.kind == "route" && !b.name.is_empty())
        .map(|(_, b)| b.name.as_str())
        .collect();
    // `route(0)` (any all-zero spelling) is the anonymous main route
    let main_exists = blocks
        .iter()
        .any(|(_, b)| b.kind == "route" && b.name.is_empty());
    let is_main_ref = |name: &str| !name.is_empty() && name.bytes().all(|c| c == b'0');
    let mut out = Vec::new();
    for r in analyze::route_refs(text) {
        if r.name.is_empty() || defined.contains(r.name.as_str()) {
            continue;
        }
        if is_main_ref(&r.name) && main_exists {
            continue;
        }
        out.push(AnalyzerDiag {
            line: r.line,
            col_start: r.col,
            col_end: r.col + r.name.len() as u32,
            message: format!(
                "route '{}' is not defined here or in included files",
                r.name
            ),
        });
    }
    // duplicate definitions across the closure, per (kind, name);
    // flag current-file ones
    let mut counts: std::collections::HashMap<(&str, &str), u32> = std::collections::HashMap::new();
    for (p, b) in &blocks {
        if b.name.is_empty() {
            continue;
        }
        let n = counts
            .entry((b.kind.as_str(), b.name.as_str()))
            .or_insert(0);
        *n += 1;
        if *n > 1 && p == path {
            out.push(AnalyzerDiag {
                line: b.line,
                col_start: b.col,
                col_end: b.col + 1,
                message: format!("route '{}' is defined more than once", b.name),
            });
        }
    }
    out.sort_by_key(|d| (d.line, d.col_start));
    out
}

/// A diagnostic re-attributed from an included file to the root.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignDiag {
    /// 0-based line in the ROOT file (the include directive, or 0).
    pub line: u32,
    /// 0-based start byte column.
    pub col_start: u32,
    /// 0-based end byte column (exclusive).
    pub col_end: u32,
    /// Message carrying the real file/line context.
    pub message: String,
}

/// The parser attributes errors inside `include_file`s to the include's
/// own path; dropping them would leave a broken config with zero
/// diagnostics.  Map such a diagnostic onto the ROOT file: at the
/// matching include directive when one resolves to `diag_file`, else
/// at the top of the file.  `diag_line` is 0-based.
pub fn attribute_foreign_diag(
    root: &std::path::Path,
    text: &str,
    diag_file: &str,
    diag_line: u32,
    message: &str,
) -> ForeignDiag {
    let short = std::path::Path::new(diag_file)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| diag_file.to_string());
    let message = format!(
        "in included file {short}, line {}: {message}",
        diag_line + 1
    );
    let target = std::path::Path::new(diag_file);
    for inc in analyze::includes(text) {
        let resolved = resolve_include(root, &inc.name);
        let hit = resolved == target
            || match (
                std::fs::canonicalize(&resolved),
                std::fs::canonicalize(target),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            };
        if hit {
            return ForeignDiag {
                line: inc.line,
                col_start: inc.col,
                // span the directive keyword
                col_end: inc.col + "include_file".len() as u32,
                message,
            };
        }
    }
    ForeignDiag {
        line: 0,
        col_start: 0,
        col_end: 1,
        message,
    }
}

/// Split a harvested signature's parameter list at TOP-LEVEL commas
/// (nested parens/brackets and quoted strings do not split):
/// `json_link($json(a), $json(b))` → two parameters, intact.
pub fn split_params(sig: &str) -> Vec<String> {
    let Some(open) = sig.find('(') else {
        return Vec::new();
    };
    let Some(close) = sig.rfind(')') else {
        return Vec::new();
    };
    if open + 1 >= close {
        return Vec::new();
    }
    let inner = &sig[open + 1..close];
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut start = 0usize;
    let b = inner.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match in_str {
            Some(q) => {
                if c == q {
                    in_str = None;
                }
            }
            None => match c {
                b'"' | b'\'' => in_str = Some(c),
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b',' if depth == 0 => {
                    out.push(inner[start..i].trim().to_string());
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    out.push(inner[start..].trim().to_string());
    out.retain(|p| !p.is_empty());
    out
}

/// Resolve the opensips binary used for `-C` diagnostics.
///
/// Order: explicit initializationOption, then environment, then a
/// bare PATH lookup.  An EXPLICIT empty string (option or env)
/// disables diagnostics entirely — required so a user can opt out of
/// running `opensips -C` (which dlopens the cfg's modules) on files
/// they do not trust.
pub fn resolve_bin(option: Option<&str>, env: Option<String>) -> Option<String> {
    match option {
        Some("") => None,
        Some(s) => Some(s.to_string()),
        None => match env {
            Some(e) if e.is_empty() => None,
            Some(e) => Some(e),
            None => Some("opensips".to_string()),
        },
    }
}

/// Does a diagnostic reported for `diag_file` belong to the file we
/// checked?  Exact path match, or same basename (tolerates symlinked
/// spellings like /tmp vs /private/tmp); an empty `diag_file` is the
/// parser's global fallback and always attaches.
pub fn diag_matches_file(diag_file: &str, checked: &std::path::Path) -> bool {
    if diag_file.is_empty() {
        return true;
    }
    let diag_path = std::path::Path::new(diag_file);
    if diag_path == checked {
        return true;
    }
    // when both paths exist, canonical identity DECIDES: symlinked
    // spellings of one file match, and an included file that merely
    // shares the basename does not cross-attach
    if let (Ok(a), Ok(b)) = (
        std::fs::canonicalize(diag_path),
        std::fs::canonicalize(checked),
    ) {
        return a == b;
    }
    // otherwise (path gone, e.g. parser echoing a deleted temp file):
    // basename is the best remaining signal
    match (diag_path.file_name(), checked.file_name()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Resolve the `opensips -C` run timeout: explicit option (ms) wins,
/// then `OPENSIPS_LSP_CHECK_TIMEOUT_MS`, then 10 seconds.  Clamped to
/// at least 1ms.
pub fn resolve_timeout(option_ms: Option<u64>, env: Option<String>) -> std::time::Duration {
    let ms = option_ms
        .or_else(|| env.and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or(10_000);
    std::time::Duration::from_millis(ms.max(1))
}

/// [`completions`] plus core-language items: core functions and core
/// parameters in code position, and pseudo-variables when the cursor
/// follows a `$`.
pub fn completions_with_core(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    line_prefix: &str,
) -> Vec<Comp> {
    let files = [(std::path::PathBuf::new(), doc.to_string())];
    complete_files(catalog, core, &files, line_prefix)
}

/// If the cursor sits in a pseudo-variable context (`$` plus an
/// alphanumeric/`.`/`_` tail reaching the cursor), the byte length of
/// the `$`-prefixed token to replace.
pub fn pvar_tail(line_prefix: &str) -> Option<usize> {
    let (_, tail) = line_prefix.rsplit_once('$')?;
    tail.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        .then_some(1 + tail.len())
}

/// Collapse duplicate labels, keeping the most informative kind
/// (Function > Param > Route > Module > Keyword); ties keep the first
/// occurrence (loaded-module items precede core items).
fn dedup_completions(items: Vec<Comp>) -> Vec<Comp> {
    fn rank(k: &CompKind) -> u8 {
        match k {
            CompKind::Function => 4,
            CompKind::Param => 3,
            CompKind::Route => 2,
            CompKind::Module => 1,
            CompKind::Keyword => 0,
        }
    }
    let mut best: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out: Vec<Option<Comp>> = Vec::with_capacity(items.len());
    for c in items {
        match best.get(&c.label) {
            Some(&i) => {
                if rank(&c.kind) > rank(&out[i].as_ref().unwrap().kind) {
                    out[i] = Some(c);
                }
            }
            None => {
                best.insert(c.label.clone(), out.len());
                out.push(Some(c));
            }
        }
    }
    out.into_iter().flatten().collect()
}

/// The signature under the cursor: the innermost UNCLOSED call in
/// `line_prefix` resolved against loaded-module functions first, then
/// any module's, then core functions.  Returns (signature, doc,
/// active-parameter index); commas inside strings do not advance the
/// index and a `#` comment ends the scan.
pub fn signature_at(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    line_prefix: &str,
) -> Option<(String, String, u32)> {
    let b = line_prefix.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut stack: Vec<(String, u32)> = Vec::new();
    let mut in_str = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'#' => break,
            b'(' => {
                let mut s = i;
                while s > 0 && (b[s - 1] as char).is_whitespace() {
                    s -= 1;
                }
                let e = s;
                while s > 0 && word(b[s - 1]) {
                    s -= 1;
                }
                let ident = std::str::from_utf8(&b[s..e]).unwrap_or("").to_string();
                stack.push((ident, 0));
            }
            b')' => {
                stack.pop();
            }
            b',' => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let (name, commas) = stack.pop()?;
    if name.is_empty() {
        return None;
    }
    let loaded: Vec<String> = analyze::loaded_modules(doc)
        .into_iter()
        .map(|m| m.name)
        .collect();
    let ordered = catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .chain(catalog.iter().filter(|m| !loaded.contains(&m.name)));
    for m in ordered {
        if let Some(f) = m.functions.iter().find(|f| f.name == name) {
            return Some((f.detail.clone(), f.doc.clone(), commas));
        }
    }
    core.functions
        .iter()
        .find(|f| f.name == name)
        .map(|f| (f.detail.clone(), f.doc.clone(), commas))
}

/// [`hover_markdown`] plus core functions, core parameters, and
/// pseudo-variables (`word` may be given without the `$`).
pub fn hover_markdown_with_core(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    word: &str,
) -> Option<String> {
    if let Some(h) = hover_markdown(catalog, doc, word) {
        return Some(h);
    }
    if let Some(f) = core.functions.iter().find(|f| f.name == word) {
        return Some(format!(
            "```\n{}\n```\n*core function*\n\n{}",
            f.detail, f.doc
        ));
    }
    if let Some(p) = core.params.iter().find(|p| p.name == word) {
        return Some(format!("**{}** — core parameter\n\n{}", p.name, p.doc));
    }
    let dollar = format!("${word}");
    if let Some(v) = core
        .pvars
        .iter()
        .find(|v| v.name == word || v.name == dollar)
    {
        return Some(format!("**{}** — {}\n\n{}", v.name, v.detail, v.doc));
    }
    None
}
