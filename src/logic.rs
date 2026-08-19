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
    "route",
    "xlog",
    "send_reply",
    "forward",
    "setflag",
    "resetflag",
    "isflagset",
    "strlen",
    "subst",
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
    re_string_arg_ctx,
    r#"(modparam\s*\(\s*"|loadmodule\s*")[^"]*$"#
);
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

    // route(<cursor>) → route names only
    if route_call_context(line_prefix) {
        return analyze::route_defs(doc)
            .into_iter()
            .filter(|r| !r.name.is_empty())
            .map(|r| Comp {
                label: r.name,
                detail: "route".into(),
                doc: String::new(),
                kind: CompKind::Route,
            })
            .collect();
    }

    // plain code: functions of LOADED modules + route names + keywords
    let loaded: Vec<String> = analyze::loaded_modules(doc)
        .into_iter()
        .map(|m| m.name)
        .collect();
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
    for r in analyze::route_defs(doc) {
        if !r.name.is_empty() {
            out.push(Comp {
                label: r.name,
                detail: "route".into(),
                doc: String::new(),
                kind: CompKind::Route,
            });
        }
    }
    for k in CORE_KEYWORDS {
        out.push(Comp {
            label: (*k).into(),
            detail: "keyword".into(),
            doc: String::new(),
            kind: CompKind::Keyword,
        });
    }
    out
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

/// Is `s` a legal route name (`[A-Za-z0-9_.:-]+`)?  Gates rename.
pub fn valid_route_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
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
    let mut out = completions(catalog, doc, line_prefix);
    // only plain code position gains core items (the string-argument
    // and route(...) contexts returned early inside `completions`)
    let in_string_ctx = analyze::modparam_context(line_prefix).is_some()
        || re_string_arg_ctx().is_match(line_prefix)
        || route_call_context(line_prefix);
    if !in_string_ctx {
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
    }
    dedup_completions(out)
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
