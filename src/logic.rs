//! Editor-feature logic, pure and testable.

use crate::analyze::{self, Located};
use crate::catalog::ModuleDoc;

#[derive(Debug, Clone, PartialEq)]
pub enum CompKind {
    Module,
    Param,
    Function,
    Route,
    Keyword,
}

#[derive(Debug, Clone)]
pub struct Comp {
    pub label: String,
    pub detail: String,
    pub doc: String,
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
    if regex::Regex::new(r#"modparam\s*\(\s*"[^"]*$"#)
        .unwrap()
        .is_match(line_prefix)
    {
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
    if regex::Regex::new(r#"loadmodule\s*"[^"]*$"#)
        .unwrap()
        .is_match(line_prefix)
    {
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
    if std::path::Path::new(diag_file) == checked {
        return true;
    }
    match (
        std::path::Path::new(diag_file).file_name(),
        checked.file_name(),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
