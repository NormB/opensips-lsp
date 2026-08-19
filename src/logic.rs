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

/// Go-to-definition: a route name under the cursor resolves to its
/// `route[name]` block.
pub fn definition_of(doc: &str, line: u32, col: u32) -> Option<Located> {
    let text_line = doc.lines().nth(line as usize)?;
    let word = analyze::word_at(text_line, col as usize)?;
    let on_ref = analyze::route_refs(doc)
        .into_iter()
        .any(|r| r.line == line && r.name == word);
    if !on_ref {
        return None;
    }
    analyze::route_defs(doc)
        .into_iter()
        .find(|d| d.name == word)
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
    if let Some(tail) = line_prefix.rsplit_once('$').map(|(_, t)| t)
        && tail
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
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
    // contexts returned early inside `completions`)
    let in_string_ctx = analyze::modparam_context(line_prefix).is_some()
        || re_string_arg_ctx().is_match(line_prefix);
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
    out
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
