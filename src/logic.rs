//! Editor-feature logic, pure and testable.

use crate::analyze::{self, Located};
use crate::catalog::{self, ModuleDoc};

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
/// Control-flow keywords that are STATEMENTS, not calls: they are
/// written `exit;`, never `exit()`.
///
/// The core documentation lists several of them among its functions,
/// so without this they complete as tabstop snippets and typing
/// `exit` yields `exit()` — wrong in a way the parser then rejects.
/// It only showed up once core docs were available by default; with a
/// configured tree it had been wrong all along.
const STATEMENT_KEYWORDS: &[&str] = &[
    "if", "else", "switch", "case", "default", "while", "for", "exit", "drop", "return", "break",
];

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
    // the tail of a `socket =` line takes modifiers, and nothing else
    // in the grammar does
    if line_prefix
        .trim_start()
        .strip_prefix("socket")
        .is_some_and(|r| r.trim_start().starts_with('='))
        && !core.socket_modifiers.is_empty()
    {
        return core
            .socket_modifiers
            .iter()
            .map(|m| Comp {
                label: m.name.clone(),
                detail: m.detail.clone(),
                doc: m.doc.clone(),
                kind: CompKind::Keyword,
            })
            .collect();
    }
    // an argument whose value is one of a fixed set the C parser knows.
    // After the `$` branch on purpose: that same parser takes a
    // pseudo-variable as the level (`s.s[0]==PV_MARKER`), so both are
    // legitimate there and the typed `$` is what says which is wanted.
    if let Some(levels) = enumerated_argument(core, line_prefix) {
        return levels;
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
        // the documented ones carry their text into the popup, so a
        // reader does not have to hover to find out what a keyword is
        let documented = core.statements.iter().find(|s| s.name == *k);
        out.push(Comp {
            label: (*k).into(),
            detail: documented.map_or_else(|| "keyword".into(), |s| s.detail.clone()),
            doc: documented.map(|s| s.doc.clone()).unwrap_or_default(),
            kind: CompKind::Keyword,
        });
    }
    for f in &core.functions {
        out.push(Comp {
            label: f.name.clone(),
            detail: f.detail.clone(),
            doc: f.doc.clone(),
            kind: if STATEMENT_KEYWORDS.contains(&f.name.as_str()) {
                CompKind::Keyword
            } else {
                CompKind::Function
            },
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

/// What extracting a selection into its own route would do.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractPlan {
    /// The generated route name, guaranteed not to collide.
    pub name: String,
    /// First selected line (0-based, inclusive).
    pub start_line: u32,
    /// Last selected line (0-based, inclusive).
    pub end_line: u32,
    /// The line that replaces the selection, at its indentation.
    pub call_line: String,
    /// 0-based line the new block is inserted before.
    pub insert_line: u32,
    /// The new `route[NAME] { ... }` block, newline-terminated.
    pub block: String,
}

/// Plan an extract-route refactoring for the lines `start..=end`, or
/// decline.
///
/// It declines far more than it accepts, and every refusal is a case
/// where accepting would change what the config does:
///
/// * outside a route block, or covering the block's own braces —
///   there is no body to lift, or lifting it would unbalance the file;
/// * unbalanced code braces inside the selection — the same;
/// * a `return` in the selection.  `return` leaves the route it is
///   written in; moved into a new route it returns to the CALLER, so
///   the statements after the extracted call would start running when
///   they did not before.  That is a behaviour change no editor should
///   make silently.
pub fn extract_route(doc: &str, start_line: u32, end_line: u32) -> Option<ExtractPlan> {
    let lines: Vec<&str> = doc.lines().collect();
    if start_line > end_line || end_line as usize >= lines.len() {
        return None;
    }
    let blocks = analyze::route_blocks(doc);
    let enclosing = enclosing_block(&blocks, start_line)?;
    // the selection must sit strictly inside the block's braces
    if start_line <= enclosing.line
        || end_line >= enclosing.end_line
        || !(start_line..=end_line).all(|l| {
            enclosing_block(&blocks, l).is_some_and(|b| {
                b.line == enclosing.line && b.name == enclosing.name && b.kind == enclosing.kind
            })
        })
    {
        return None;
    }

    let selected: Vec<&str> = lines[start_line as usize..=end_line as usize].to_vec();
    if selected.iter().all(|l| l.trim().is_empty()) {
        return None;
    }

    // brace balance and `return`, judged in code position only
    let class = crate::analyze::classify(doc);
    let mut off = doc
        .lines()
        .take(start_line as usize)
        .map(|l| l.len() + 1)
        .sum::<usize>();
    let mut depth = 0i32;
    for line in &selected {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let code = class.get(off + i) == Some(&crate::analyze::Class::Code);
            if code {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                if depth < 0 {
                    return None;
                }
                if bytes[i..].starts_with(b"return")
                    && (i == 0 || !crate::analyze::is_word_byte(bytes[i - 1]))
                    && bytes
                        .get(i + 6)
                        .is_none_or(|c| !crate::analyze::is_word_byte(*c))
                {
                    return None;
                }
            }
            i += 1;
        }
        off += line.len() + 1;
    }
    if depth != 0 {
        return None;
    }

    // a name nothing else uses
    let taken: std::collections::HashSet<String> = blocks.iter().map(|b| b.name.clone()).collect();
    let mut name = "EXTRACTED".to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("EXTRACTED_{n}");
        n += 1;
    }

    let first = selected[0];
    let indent = &first[..first.len() - first.trim_start().len()];
    let mut block = format!("route[{name}] {{\n");
    for line in &selected {
        block.push_str(line);
        block.push('\n');
    }
    block.push_str("}\n");

    Some(ExtractPlan {
        call_line: format!("{indent}route({name});"),
        name,
        start_line,
        end_line,
        insert_line: enclosing.end_line + 1,
        block,
    })
}

/// The lines carrying a `loadmodule` that an earlier line already
/// loaded — every occurrence after the first, per module.
///
/// Loading a module twice is not a harmless tidy-up issue: the real
/// parser rejects the second load outright, so these lines are an
/// error the document can be repaired of.
pub fn duplicate_loadmodules(doc: &str) -> Vec<u32> {
    let mut seen = std::collections::HashSet::new();
    analyze::loaded_modules(doc)
        .into_iter()
        .filter(|m| !seen.insert(m.name.clone()))
        .map(|m| m.line)
        .collect()
}

/// One inlay hint: a label the editor draws at a position, without
/// changing the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    /// 0-based line.
    pub line: u32,
    /// 0-based byte column the label is drawn before.
    pub col: u32,
    /// The label as drawn, already punctuated.
    pub label: String,
}

/// The parameter name to draw for one signature parameter.
///
/// Signatures are written for humans — `[flags]`, `[outbound_proxy]`,
/// sometimes with a type in front — so the bracket markers and any
/// leading type are stripped down to the name itself.  A parameter
/// that reduces to nothing gets no hint at all rather than an empty
/// chip.
fn hint_name(param: &str) -> Option<String> {
    let p = param.trim().trim_matches(['[', ']']).trim();
    let p = p.split('=').next().unwrap_or(p).trim();
    let p = p.split_whitespace().last().unwrap_or(p);
    let p = p.trim_matches(['[', ']', '"', '\'']);
    (!p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| p.to_string())
}

/// The signature of `name`, preferring the modules the document
/// actually loads, then any module, then the core functions — the
/// same order hover resolves in.
fn signature_of(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    name: &str,
) -> Option<String> {
    let loaded = loaded_names(doc);
    let sig = |m: &ModuleDoc| {
        m.functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.detail.clone())
    };
    catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .find_map(sig)
        .or_else(|| {
            core.functions
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.detail.clone())
        })
        .or_else(|| {
            catalog
                .iter()
                .filter(|m| !loaded.contains(&m.name))
                .find_map(sig)
        })
}

/// Parameter-name hints for every documented call in `doc`.
///
/// Only calls the catalogue knows get hints, which is what keeps
/// keywords (`if`, `while`, `route`) out without special-casing them.
/// A call with more arguments than the signature documents is hinted
/// as far as the signature goes and no further — guessing past the
/// end would be inventing names.
pub fn parameter_hints(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
) -> Vec<Hint> {
    let mut out = Vec::new();
    for call in analyze::calls(doc) {
        if call.args.is_empty() {
            continue;
        }
        let Some(sig) = signature_of(catalog, core, doc, &call.name) else {
            continue;
        };
        let params = split_params(&sig);
        for (param, (line, col)) in params.iter().zip(call.args.iter()) {
            if let Some(name) = hint_name(param) {
                out.push(Hint {
                    line: *line,
                    col: *col,
                    label: format!("{name}:"),
                });
            }
        }
    }
    out
}

/// The modules `doc` loads, by name.
fn loaded_names(doc: &str) -> Vec<String> {
    analyze::loaded_modules(doc)
        .into_iter()
        .map(|m| m.name)
        .collect()
}

/// Hover text for `word` among the given modules, if one documents it.
fn hover_among<'a>(mods: impl Iterator<Item = &'a ModuleDoc>, word: &str) -> Option<String> {
    for m in mods {
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
    None
}

/// Markdown hover for `word`: loaded-module symbols win, then any
/// module's symbols, then module names themselves.
///
/// Core entries are not consulted here — [`hover_markdown_with_core`]
/// interleaves them, because a core global must outrank a same-named
/// parameter of a module the config never loads.
pub fn hover_markdown(catalog: &[ModuleDoc], doc: &str, word: &str) -> Option<String> {
    let loaded = loaded_names(doc);
    hover_among(catalog.iter().filter(|m| loaded.contains(&m.name)), word)
        .or_else(|| hover_among(catalog.iter().filter(|m| !loaded.contains(&m.name)), word))
        .or_else(|| hover_module_name(catalog, word))
}

/// Hover for a module name itself.
fn hover_module_name(catalog: &[ModuleDoc], word: &str) -> Option<String> {
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
    // only main-table blocks are route() targets: failure_route[x]
    // and friends are armed via module functions (t_on_failure)
    analyze::route_blocks(doc)
        .into_iter()
        .filter(|b| b.kind == "route")
        .map(|b| Located {
            name: b.name,
            line: b.line,
            col: b.col,
        })
        .find(|d| d.name == name)
}

/// The innermost route-family block whose extent covers `line`.
///
/// Route blocks do not nest in this language, so at most one can
/// match; the search is over `blocks` rather than the text so a
/// caller that already has them does not pay to re-scan.
pub fn enclosing_block(blocks: &[analyze::Block], line: u32) -> Option<&analyze::Block> {
    blocks.iter().find(|b| line >= b.line && line <= b.end_line)
}

/// Every `route(NAME)` call in `doc`, paired with the block it sits
/// in.  These are the edges of the route call graph.
///
/// The target of an edge is always a main-table `route[NAME]` block —
/// that is the route-namespace rule — but the source can be any
/// route-family block: a `failure_route` is not itself callable via
/// `route()`, yet it may call into the main table, and that is a real
/// edge.  A call outside every block carries `None`; the real parser
/// rejects such a config, but the analyzer must not assume it.
pub fn call_edges(doc: &str) -> Vec<(Option<analyze::Block>, Located)> {
    let blocks = analyze::route_blocks(doc);
    analyze::route_refs(doc)
        .into_iter()
        .map(|call| (enclosing_block(&blocks, call.line).cloned(), call))
        .collect()
}

/// The namespace a route-name symbol lives in.  OpenSIPS keeps one
/// route table per block kind: `route(NAME)` invokes only the main
/// table (`route[NAME]` blocks); `failure_route[NAME]` and friends
/// are armed through module functions (`t_on_failure("NAME")`) and
/// share nothing with the main table — same-name cross-kind configs
/// are legal.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteNs {
    /// The main route table: `route[NAME]` defs + `route(NAME)` calls.
    Main,
    /// A per-kind table (`failure_route`, `event_route`, ...): just
    /// that kind's bracket names.
    Kind(String),
}

/// The route-name symbol whose span covers byte position (line, col)
/// — at a `route(name)` call site or at the NAME inside any named
/// block — together with its namespace.  Unnamed blocks have no
/// symbol.
pub fn route_symbol_ns_at(doc: &str, line: u32, col: u32) -> Option<(String, RouteNs)> {
    if let Some(r) = analyze::route_refs(doc)
        .into_iter()
        .find(|r| in_span(r, &r.name, line, col))
    {
        return Some((r.name, RouteNs::Main));
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
        .map(|b| {
            let ns = if b.kind == "route" {
                RouteNs::Main
            } else {
                RouteNs::Kind(b.kind.clone())
            };
            (b.name, ns)
        })
}

/// Every occurrence of `name` within its namespace in `doc`.  The
/// bool is `true` for definitions.  Main-namespace occurrences are
/// `route(NAME)` call sites plus `route[NAME]` definitions; per-kind
/// namespaces list only that kind's definition name spans (their call
/// sites are strings inside module functions like `t_on_failure`).
pub fn ns_occurrences(doc: &str, name: &str, ns: &RouteNs) -> Vec<(Located, bool)> {
    if name.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(Located, bool)> = Vec::new();
    if *ns == RouteNs::Main {
        out.extend(
            analyze::route_refs(doc)
                .into_iter()
                .filter(|r| r.name == name)
                .map(|r| (r, false)),
        );
    }
    let want_kind = match ns {
        RouteNs::Main => "route",
        RouteNs::Kind(k) => k.as_str(),
    };
    for b in analyze::route_blocks(doc) {
        if b.name == name && b.kind == want_kind {
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

/// [`route_symbol_ns_at`], name only.
pub fn route_symbol_at(doc: &str, line: u32, col: u32) -> Option<String> {
    route_symbol_ns_at(doc, line, col).map(|(n, _)| n)
}

/// Main-namespace occurrences of route `name` in `doc`.
pub fn route_occurrences(doc: &str, name: &str) -> Vec<(Located, bool)> {
    ns_occurrences(doc, name, &RouteNs::Main)
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

/// Fold `.` and `..` components textually.
///
/// A split-by-site layout reaches shared routing as
/// `../common/routing.cfg`, which resolves to
/// `<ws>/sites/../common/routing.cfg` — the same file the editor
/// opens as `<ws>/common/routing.cfg`, under a different name.  Two
/// names for one file mean a fragment with no root and an include
/// closure that visits it twice, so every route it defines reads as
/// defined more than once.
///
/// Folded textually rather than through `canonicalize`, because an
/// include may name a file that does not exist yet (document links
/// are produced for those) and because canonicalising would replace
/// the path the user sees with the target of any symlink on it.  The
/// cost is the one case where the two differ: if `sites` is itself a
/// symlink, the OS reads `sites/../common` relative to the LINK's
/// target and this does not.
fn lexically_normal(p: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // `/..` is `/` on POSIX; anything else (nothing yet, or
                // a `..` already kept) has nothing to fold into
                Some(Component::RootDir) => {}
                _ => out.push(c.as_os_str()),
            },
            _ => out.push(c.as_os_str()),
        }
    }
    out
}

/// Resolve an `include_file`/`import_file` path: absolute paths as
/// written, relative paths against the INCLUDING file's directory.
/// `.` and `..` are folded so one file has one name — see
/// [`lexically_normal`].
fn resolve_include(from: &std::path::Path, inc: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(inc);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        from.parent()
            .unwrap_or_else(|| std::path::Path::new(""))
            .join(p)
    };
    lexically_normal(&joined)
}

/// Extensions the sweep will open looking for a configuration.
///
/// A split OpenSIPS tree names its fragments whatever its author felt
/// like — `.inc` is the common one, `.m4` is what a templated tree
/// uses — and the root of such a tree need not be a `.cfg` at all.
/// Restricted to `.cfg` the sweep could not see those trees, so no
/// fragment in them ever resolved a root. This is deliberately a
/// short list rather than "every file": the sweep walks a whole
/// workspace folder, and reading all of it is not a search, it is a
/// scan of the user's disk.
const CONFIG_EXTENSIONS: [&str; 3] = ["cfg", "inc", "m4"];

/// Whether the sweep should treat `path` as a possible configuration.
fn is_config_candidate(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| CONFIG_EXTENSIONS.contains(&x))
}

/// The configuration files sitting DIRECTLY in `dir` — no recursion.
///
/// For looking one level up: a fragment in `inc/` is usually included
/// by a config in the directory above it, and reading that directory's
/// own files is cheap, while walking it recursively could be a scan of
/// `/etc` or worse.
pub fn configs_in_dir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_config_candidate(p))
        .take(200)
        .collect()
}

/// Every configuration file under `roots`, bounded.
///
/// The server scans the client's workspace folders; the CLI scans the
/// directory holding the files it was given.  Same walk, so the two
/// cannot come to different conclusions about which file is part of
/// which — the bound is announced by the caller rather than applied
/// silently.
pub fn scan_configs(roots: &[std::path::PathBuf], limit: usize) -> (Vec<std::path::PathBuf>, bool) {
    let mut out = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = roots.to_vec();
    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    while let Some(dir) = stack.pop() {
        // a directory symlink can point back up the tree
        if !seen.insert(std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone())) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if is_config_candidate(&path) {
                if out.len() >= limit {
                    return (out, true);
                }
                out.push(path);
            }
        }
    }
    (out, false)
}

/// The most a single config may contribute to an analysis.
///
/// Documented as the per-file include bound, so it belongs to the
/// reading, not to one caller of it.
pub const MAX_CONFIG_BYTES: u64 = 1_048_576;

/// Read a config for analysis: one definition, used by the workspace
/// scan and the include loader alike.
///
/// Two definitions are a disagreement, and this one disagreed twice.
/// The scan read any size and demanded valid UTF-8; the loader capped
/// at 1 MiB and demanded the same.  So an oversized root was FOUND by
/// the scan and then could not be loaded — its routes silently left
/// scope while its fragments still claimed it — and one byte that is
/// not UTF-8, a latin-1 accent in a comment, erased a config from the
/// graph along with every fragment it includes.
///
/// Bytes are decoded lossily on purpose.  A comment nobody will parse
/// is not a reason to lose the file, and the text whose columns are
/// ever REPORTED comes from the editor's own buffer, never from here.
pub fn read_config(path: &std::path::Path) -> Option<String> {
    let md = std::fs::metadata(path).ok()?;
    if !md.is_file() || md.len() > MAX_CONFIG_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The files one config includes DIRECTLY, resolved against it.
///
/// Callers use this to notice that an edit added or removed an
/// include: which file is a fragment of which is the only thing an
/// edit can change about the graph, and it is far cheaper to compare
/// two of these than to rebuild it.
pub fn resolved_includes(path: &std::path::Path, text: &str) -> Vec<std::path::PathBuf> {
    analyze::includes(text)
        .into_iter()
        .map(|inc| resolve_include(path, &inc.name))
        .collect()
}

/// The workspace's include graph, inverted: for each config, the
/// configs that name it in an `include_file`/`import_file` directive.
///
/// Transitivity needs no traversal to record: every config in the
/// scan contributes its own directives, so a fragment three levels
/// down has the fragment above it as a parent and
/// [`Self::analysis_root`] climbs one edge at a time.
#[derive(Debug, Default, Clone)]
pub struct IncludeGraph {
    parents: std::collections::BTreeMap<std::path::PathBuf, Vec<std::path::PathBuf>>,
}

impl IncludeGraph {
    /// Build from a workspace scan: one `(path, text)` per config.
    pub fn build(configs: &[(std::path::PathBuf, String)]) -> Self {
        let mut parents: std::collections::BTreeMap<std::path::PathBuf, Vec<std::path::PathBuf>> =
            std::collections::BTreeMap::new();
        for (path, text) in configs {
            // Every include, conditional or not: OpenSIPS 4.0.1 reads
            // an `include_file` inside an unmet `#!ifdef` anyway —
            // pinned by `the_real_parser_reads_a_conditional_include_anyway`
            // in the proof suite.  Kamailio differs, and its sibling
            // skips them for that reason; copying the rule across
            // would stop claiming roots that genuinely do include the
            // file.
            for inc in analyze::includes(text) {
                let entry = parents.entry(resolve_include(path, &inc.name)).or_default();
                if !entry.contains(path) {
                    entry.push(path.clone());
                }
            }
        }
        // sorted so the pick below cannot depend on scan order
        for v in parents.values_mut() {
            v.sort();
        }
        Self { parents }
    }

    /// The config `path` should be analysed as part of: the top of the
    /// include chain that reaches it, or `None` when nothing includes
    /// it — it is a program in its own right, or the scan never saw
    /// it.
    ///
    /// A fragment reached from more than one root has no single true
    /// answer; the lexicographically first parent is taken at every
    /// step, so the context a fragment is analysed in cannot flicker
    /// as the scan order changes.  A cycle stops the climb at the last
    /// config not already visited.
    pub fn analysis_root(&self, path: &std::path::Path) -> Option<std::path::PathBuf> {
        let mut seen: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        // the keys were folded when the graph was built; a caller's
        // path has to be folded the same way or it cannot match
        let path = lexically_normal(path);
        seen.insert(path.clone());
        let mut best: Option<std::path::PathBuf> = None;
        let mut cur = path;
        loop {
            let Some(next) = self.parents.get(&cur).and_then(|v| v.first()) else {
                return best;
            };
            if !seen.insert(next.clone()) {
                return best;
            }
            cur = next.clone();
            best = Some(next.clone());
        }
    }
}

/// Canonicalise a path when it exists, take it as written otherwise.
fn norm_path(p: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The one line a document carries when a `opensips -C` run FAILED but
/// nothing it reported belongs to that document.
///
/// The parser does not always position an error — a module it cannot
/// load, a bad module path — and a note built from a diagnostic that
/// carries no file renders as `check failed in , line 1: ...`, which
/// names an empty file and sends the reader to a line the parser
/// never mentioned.  Say only what is known.
pub fn check_failure_note(first: Option<&crate::diag::Diag>, rc: i32) -> String {
    match first {
        Some(d) if !d.file.is_empty() => format!(
            "check failed in {}, line {}: {}",
            d.file,
            d.line + 1,
            d.message
        ),
        Some(d) => format!("check failed: {}", d.message),
        None => format!("check failed (rc={rc})"),
    }
}

/// Route one `opensips -C` diagnostic to an open FRAGMENT.
///
/// The checker only accepts a whole program, so a fragment on screen
/// is checked through its root ([`IncludeGraph::analysis_root`]) and
/// `checked` is that root.  A diagnostic naming the fragment is the
/// one the fragment's buffer should carry, at its own line — not
/// folded onto an `include_file` directive the fragment does not
/// contain.  Everything else belongs to the root or to a sibling
/// fragment: `None`, so the caller can decide once, for the whole
/// run, that the program is broken elsewhere.
///
/// The file spelling is resolved as written, against the checked
/// file's directory (the subprocess runs there), and against our own
/// cwd.  Empty and NUL-bearing spellings name nothing.
pub fn fragment_check_diag(
    checked: &std::path::Path,
    reported: &std::path::Path,
    d: &crate::diag::Diag,
) -> Option<crate::diag::Diag> {
    if d.file.is_empty() || d.file.contains('\0') {
        return None;
    }
    let diag_path = std::path::Path::new(&d.file);
    let mut cands: Vec<std::path::PathBuf> = vec![norm_path(diag_path)];
    if diag_path.is_relative() {
        let dir = checked.parent().unwrap_or_else(|| std::path::Path::new(""));
        cands.push(norm_path(&dir.join(diag_path)));
        if let Ok(cwd) = std::env::current_dir() {
            cands.push(norm_path(&cwd.join(diag_path)));
        }
    }
    cands
        .contains(&norm_path(reported))
        .then(|| crate::diag::Diag {
            file: reported.display().to_string(),
            ..d.clone()
        })
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

/// Preprocessor directives Kamailio's `src/core/cfg.lex` defines and
/// OpenSIPS does not have at all.
///
/// OpenSIPS's own `cfg.lex` has `COM_LINE #` and the rule
/// `<INITIAL>{COM_LINE}.*{CR} { count(); }` — a `#` starts a line
/// comment that is counted and thrown away — and contains no
/// preprocessor token of any kind.  So `#!ifdef USE_TCP` does not
/// guard anything: the block it appears to open is always active, and
/// `#!define X 5060` binds nothing.  Proven against the real 4.0.1
/// binary: a syntax error placed inside `#!ifdef NEVER_DEFINED` is
/// still reported, which it would not be if the block were skipped.
///
/// That silence is the problem.  A config carried over from Kamailio
/// keeps working in the sense that it parses, while every conditional
/// in it has quietly stopped meaning anything.
const KAMAILIO_ONLY_DIRECTIVES: &[&str] = &[
    "def",
    "define",
    "defenv",
    "defenvs",
    "defexp",
    "defexps",
    "endif",
    "ifdef",
    "ifexp",
    "ifndef",
    "redef",
    "redefine",
    "subst",
    "substdef",
    "substdefs",
    "trydef",
    "trydefenv",
    "trydefenvs",
    "trydefine",
];

/// Warn on `#!` directives that do nothing here.
///
/// Only a comment that STARTS at this position counts: the same text
/// inside a block comment is somebody deliberately commenting code
/// out, and flagging that would be noise.
fn inert_directive_diagnostics(text: &str) -> Vec<AnalyzerDiag> {
    let classes = analyze::classify(text);
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let lead = line.len() - line.trim_start().len();
        let at = offset + lead;
        let rest = line.trim_start();
        offset += line.len();
        let Some(after) = rest.strip_prefix("#!") else {
            continue;
        };
        // a comment that begins here, not one already open
        if classes.get(at) != Some(&analyze::Class::Comment)
            || (at > 0 && classes.get(at - 1) == Some(&analyze::Class::Comment))
        {
            continue;
        }
        let kw: String = after
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if kw.is_empty() || !KAMAILIO_ONLY_DIRECTIVES.contains(&kw.to_ascii_lowercase().as_str()) {
            continue;
        }
        let (line_no, _) = byte_line_col(text, at);
        out.push(AnalyzerDiag {
            line: line_no,
            col_start: lead as u32,
            col_end: (lead + rest.trim_end_matches(['\r', '\n']).len()) as u32,
            message: format!(
                "`#!{kw}` has no effect: OpenSIPS has no preprocessor, so this is \
                 a comment. It is a Kamailio directive — any block it appears to \
                 guard is always active here"
            ),
        });
    }
    out
}

/// Line and column of a byte offset.
fn byte_line_col(text: &str, at: usize) -> (u32, u32) {
    let before = &text[..at];
    let line = before.matches('\n').count() as u32;
    let col = before.len() - before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    (line, col as u32)
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
    analyzer_diagnostics_in_closure(&files, path, text)
}

/// [`analyzer_diagnostics`] against a closure the caller already has.
///
/// The closure is the unit of truth and it need not be rooted at
/// `path`: an included FRAGMENT is analysed in the closure of its
/// root, which is the only context in which the routes its parent
/// defines exist.  Reported positions still come from `text` and
/// belong to `path` alone.
pub fn analyzer_diagnostics_in_closure(
    files: &[(std::path::PathBuf, String)],
    path: &std::path::Path,
    text: &str,
) -> Vec<AnalyzerDiag> {
    let mut out = inert_directive_diagnostics(text);
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

/// The clause naming other supported releases that export this
/// parameter, or nothing when there are none.
///
/// Only meaningful for the built-in catalogue: a configured source
/// tree is one release by construction, and this server knows no
/// others to compare it against.
fn elsewhere(origin: &catalog::CatalogOrigin, module: &str, param: &str) -> String {
    let catalog::CatalogOrigin::BuiltIn(current) = origin else {
        return String::new();
    };
    let others: Vec<String> = catalog::builtin_versioned()
        .versions_with_param(module, param)
        .into_iter()
        .filter(|v| v != current)
        .collect();
    if others.is_empty() {
        return String::new();
    }
    format!(" — it exists in {}", others.join(", "))
}

/// Catalog-pinned validation: flag `modparam("m", "p", ...)` where
/// the configured source tree documents module `m` but no parameter
/// `p`.  Version-exact by construction — the catalog IS the user's
/// pinned tree; unknown modules, and modules whose parameters were
/// never harvested, stay silent (the catalog may simply not cover
/// them).  Doc-derived, so kept separate from the grammar-derived
/// [`analyzer_diagnostics`].
pub fn catalog_diagnostics(
    catalog: &[ModuleDoc],
    origin: &catalog::CatalogOrigin,
    text: &str,
) -> Vec<AnalyzerDiag> {
    if catalog.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for call in analyze::modparam_calls(text) {
        let Some(m) = catalog.iter().find(|m| m.name == call.module) else {
            continue;
        };
        // A module the harvester read NOTHING from documents
        // nothing, and an empty list is then no evidence that a
        // parameter does not exist.  `auth_web3` writes its README in
        // a shape the harvester does not read, and it is still a
        // module a config can load.  A module with functions but no
        // parameters WAS read, and really exports none.
        if m.params.is_empty() && m.functions.is_empty() {
            continue;
        }
        if m.params.iter().any(|p| p.name == call.param) {
            continue;
        }
        out.push(AnalyzerDiag {
            line: call.line,
            col_start: call.col,
            col_end: call.col + call.param.len() as u32,
            // Name the catalogue, and say when the name exists in
            // another supported release. A parameter absent from the
            // release in use but present in a neighbouring one is
            // almost never a typo — it is a version mismatch, and
            // that is a different thing for the reader to go and do.
            message: format!(
                "parameter '{}' is not exported by module '{}' in {}{}",
                call.param,
                call.module,
                origin.describe(),
                elsewhere(origin, &call.module, &call.param)
            ),
        });
    }
    out
}

/// Semantic-token categories the server emits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SemKind {
    /// A route name at a definition or call site (legend index 0).
    RouteName,
    /// A pseudo-variable (legend index 1).
    Pvar,
}

/// One semantic span, in BYTE columns on its line.
#[derive(Debug, Clone, PartialEq)]
pub struct SemSpan {
    /// 0-based line.
    pub line: u32,
    /// 0-based start byte column.
    pub col: u32,
    /// Byte length.
    pub len: u32,
    /// Category.
    pub kind: SemKind,
}

/// Semantic spans for a document: route names (definitions and call
/// sites) and pseudo-variables.  Pvars inside strings count —
/// OpenSIPS interpolates them there; comments (line or block,
/// classified byte-by-byte) never do.
pub fn semantic_spans(text: &str) -> Vec<SemSpan> {
    let mut out = Vec::new();
    for b in analyze::route_blocks(text) {
        if !b.name.is_empty() {
            out.push(SemSpan {
                line: b.name_line,
                col: b.name_col,
                len: b.name.len() as u32,
                kind: SemKind::RouteName,
            });
        }
    }
    for r in analyze::route_refs(text) {
        out.push(SemSpan {
            line: r.line,
            col: r.col,
            len: r.name.len() as u32,
            kind: SemKind::RouteName,
        });
    }
    for (line, col, len) in analyze::pvars(text) {
        out.push(SemSpan {
            line,
            col,
            len,
            kind: SemKind::Pvar,
        });
    }
    out.sort_by_key(|s| (s.line, s.col));
    out.dedup_by_key(|s| (s.line, s.col));
    out
}

/// LSP semanticTokens/full data: delta-encoded quintuples with
/// UTF-16 columns and lengths.
pub fn encode_semantic_tokens(text: &str) -> Vec<u32> {
    encode_span_list(text, semantic_spans(text))
}

/// [`encode_semantic_tokens`] restricted to an LSP range (UTF-16
/// boundaries): only spans lying fully inside the range are encoded,
/// with deltas restarting from the document origin per the LSP spec.
pub fn encode_semantic_tokens_range(text: &str, sl: u32, sc: u32, el: u32, ec: u32) -> Vec<u32> {
    let lines: Vec<&str> = text.lines().collect();
    let spans = semantic_spans(text)
        .into_iter()
        .filter(|s| {
            let Some(line) = lines.get(s.line as usize) else {
                return false;
            };
            let start = analyze::byte_to_utf16(line, s.col as usize);
            let end = analyze::byte_to_utf16(line, (s.col + s.len) as usize);
            (s.line > sl || (s.line == sl && start >= sc))
                && (s.line < el || (s.line == el && end <= ec))
        })
        .collect();
    encode_span_list(text, spans)
}

fn encode_span_list(text: &str, spans: Vec<SemSpan>) -> Vec<u32> {
    let lines: Vec<&str> = text.lines().collect();
    let mut data = Vec::new();
    let (mut prev_line, mut prev_start) = (0u32, 0u32);
    for s in spans {
        let Some(line) = lines.get(s.line as usize) else {
            continue;
        };
        let start = analyze::byte_to_utf16(line, s.col as usize);
        let end = analyze::byte_to_utf16(line, (s.col + s.len) as usize);
        let delta_line = s.line - prev_line;
        let delta_start = if delta_line == 0 {
            start - prev_start
        } else {
            start
        };
        data.extend_from_slice(&[
            delta_line,
            delta_start,
            end - start,
            match s.kind {
                SemKind::RouteName => 0,
                SemKind::Pvar => 1,
            },
            0,
        ]);
        prev_line = s.line;
        prev_start = start;
    }
    data
}

/// One quick-fix: a single text insertion with a title.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickFix {
    /// Human-readable action title.
    pub title: String,
    /// 0-based insertion line.
    pub line: u32,
    /// 0-based insertion column.
    pub col: u32,
    /// Text to insert.
    pub insert: String,
}

static_regex!(re_unknown_cmd, r"unknown command <([A-Za-z0-9_]+)>");
static_regex!(
    re_undefined_route,
    r"route '([A-Za-z0-9_.:-]+)' is not defined"
);

/// Quick fixes for a diagnostic `message` on `doc`:
/// - `unknown command <f>` → load the module that exports `f` (one
///   action per exporting module, skipped when already loaded);
/// - `route 'x' is not defined` → append a `route[x]` stub.
pub fn quick_fixes(catalog: &[ModuleDoc], doc: &str, message: &str) -> Vec<QuickFix> {
    let mut out = Vec::new();
    if let Some(c) = re_unknown_cmd().captures(message) {
        let f = &c[1];
        let loaded: Vec<String> = analyze::loaded_modules(doc)
            .into_iter()
            .map(|m| m.name)
            .collect();
        // insertion point: right after the LAST loadmodule line
        let insert_line = analyze::loaded_modules(doc)
            .into_iter()
            .map(|m| m.line + 1)
            .max()
            .unwrap_or(0);
        for m in catalog {
            if loaded.contains(&m.name) || !m.functions.iter().any(|x| x.name == f) {
                continue;
            }
            out.push(QuickFix {
                title: format!("Load module '{}' (exports {f})", m.name),
                line: insert_line,
                col: 0,
                insert: format!("loadmodule \"{}.so\"\n", m.name),
            });
        }
    }
    if let Some(c) = re_undefined_route().captures(message) {
        let name = &c[1];
        out.push(QuickFix {
            title: format!("Create route[{name}]"),
            line: doc.lines().count() as u32,
            col: 0,
            insert: format!("\nroute[{name}] {{\n\texit;\n}}\n"),
        });
    }
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
                let kept = out[i].as_ref().unwrap();
                // On equal rank the documented one wins.  `exit` is
                // offered twice — once as a bare keyword, once from
                // the core docs — and both are keywords, so without
                // this the doc-less one survives purely by arriving
                // first and hover comes back empty.
                let better = rank(&c.kind) > rank(&kept.kind)
                    || (rank(&c.kind) == rank(&kept.kind)
                        && kept.doc.is_empty()
                        && !c.doc.is_empty());
                if better {
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

/// Whether `line` of `doc` is a `socket =` assignment — the only
/// statement in the grammar that takes socket modifiers.
fn on_a_socket_line(doc: &str, line: u32) -> bool {
    doc.lines()
        .nth(line as usize)
        .map(|l| {
            let t = l.trim_start();
            t.strip_prefix("socket")
                .is_some_and(|r| r.trim_start().starts_with('='))
        })
        .unwrap_or(false)
}

/// The call the cursor sits in: the function named before the open
/// parenthesis, which argument the cursor is in, and whether it is
/// inside a string literal.
///
/// Both signature help and the enumerated-argument offers need this
/// walk, and they must agree about what argument the cursor is in —
/// two walks would be two answers.
pub struct CallSite {
    /// The identifier before the open parenthesis.
    pub name: String,
    /// Zero-based argument index: the commas seen at this depth.
    pub arg: u32,
    /// The cursor is inside a string literal.
    pub in_string: bool,
}

/// The innermost call the cursor sits in, or `None` outside one.
pub fn call_site(line_prefix: &str) -> Option<CallSite> {
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
                let mut e = i;
                while e > 0 && (b[e - 1] as char).is_whitespace() {
                    e -= 1;
                }
                let mut st = e;
                while st > 0 && word(b[st - 1]) {
                    st -= 1;
                }
                let ident = std::str::from_utf8(&b[st..e]).unwrap_or("").to_string();
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
    let (name, arg) = stack.pop()?;
    Some(CallSite {
        name,
        arg,
        in_string: in_str,
    })
}

/// The offers for an argument whose value is one of a fixed set the
/// server's own C parser recognises.
///
/// The grammar spells the argument `STRING`, so `xlog(L_INFO, ...)`
/// is a syntax error: the quotes belong to the completion. When the
/// reader has already typed the opening quote, adding another pair
/// would give `""L_INFO"`, so the bare form is offered there instead.
fn enumerated_argument(core: &crate::catalog::CoreDocs, line_prefix: &str) -> Option<Vec<Comp>> {
    let site = call_site(line_prefix)?;
    if !LEVEL_ARGUMENTS
        .iter()
        .any(|(f, a)| *f == site.name && *a == site.arg)
    {
        return None;
    }
    Some(
        core.log_levels
            .iter()
            .map(|l| Comp {
                label: if site.in_string {
                    l.clone()
                } else {
                    format!("\"{l}\"")
                },
                detail: "log level".into(),
                doc: String::new(),
                kind: CompKind::Keyword,
            })
            .collect(),
    )
}

/// Which argument of which call is a log level. `xlog(level, format)`
/// is the two-argument form; the one-argument form is a format alone,
/// and the offer at argument 0 serves both — a reader who wanted the
/// short form types their format instead.
const LEVEL_ARGUMENTS: &[(&str, u32)] = &[("xlog", 0)];

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

/// Where the hovered word sits, when the syntax decides the answer on
/// its own rather than leaving it to precedence.
///
/// A global `log_level=2` and `modparam("opentelemetry", "log_level",
/// 2)` are two different things that share a name, and the config says
/// which one you are looking at.  Resolving by precedence answers the
/// question the text already answered: with opentelemetry loaded, the
/// global assignment hovered as the module's parameter.
enum HoverSite {
    /// The parameter-name argument of `modparam("m", "p", ...)`: the
    /// module named in the call decides, not the loaded set.
    Modparam(String),
    /// A bare `name = value` statement — a core global.  Inside a
    /// route, assignment targets are pseudo-variables, which are not
    /// bare words, so this form is unambiguous.
    Global,
    /// Anywhere else: a call, an operand, a module name.
    Other,
}

fn hover_site(doc: &str, word: &str, line: u32, col: u32) -> HoverSite {
    if let Some(c) = analyze::modparam_calls(doc).into_iter().find(|c| {
        c.line == line && c.param == word && col >= c.col && col < c.col + c.param.len() as u32
    }) {
        return HoverSite::Modparam(c.module);
    }
    let Some(text) = doc.lines().nth(line as usize) else {
        return HoverSite::Other;
    };
    let lead = (text.len() - text.trim_start().len()) as u32;
    let rest = text.trim_start();
    let starts_here = col >= lead && col < lead + word.len() as u32;
    if starts_here && rest.starts_with(word) && rest[word.len()..].trim_start().starts_with('=') {
        return HoverSite::Global;
    }
    HoverSite::Other
}

/// [`hover_markdown_with_core`], with the syntax consulted first.
pub fn hover_markdown_at(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    word: &str,
    line: u32,
    col: u32,
) -> Option<String> {
    // A socket modifier: `socket = udp:1.2.3.4:5060 use_workers 4`.
    // Scoped to the line, because only `socket =` takes them and the
    // words themselves — `as`, `tag`, `frag` — are ordinary. A hover
    // that fires on every `tag` in a configuration is worse than one
    // that fires on none.
    if on_a_socket_line(doc, line)
        && let Some(m) = core.socket_modifiers.iter().find(|m| m.name == word)
    {
        return Some(format!("**{}** — {}\n\n{}", m.name, m.detail, m.doc));
    }
    // A control statement. These are keywords rather than calls, so
    // nothing else in the lookup below would ever answer for them:
    // they completed with `detail: "keyword"` and no text at all, and
    // hovered nothing.
    if let Some(st) = core.statements.iter().find(|s| s.name == word) {
        return Some(format!("**{}** — {}\n\n{}", st.name, st.detail, st.doc));
    }
    // A route type: the kind of block a configuration is built out
    // of. `route` is the one that is also a core function — at a
    // definition the block is what the reader is looking at, at a
    // call site the function is. The others have no second meaning,
    // so they answer wherever they appear.
    if let Some(r) = core.routes.iter().find(|r| r.name == word) {
        let also_a_function = core.functions.iter().any(|f| f.name == word);
        let at_a_definition = analyze::route_defs(doc).iter().any(|d| d.line == line);
        if !also_a_function || at_a_definition {
            return Some(format!("**{}** — {}\n\n{}", r.name, r.detail, r.doc));
        }
    }
    match hover_site(doc, word, line, col) {
        HoverSite::Modparam(module) => {
            // the module the call names, and only it
            if let Some(h) = hover_among(catalog.iter().filter(|m| m.name == module), word) {
                return Some(h);
            }
            // an undocumented module, or a parameter it does not
            // declare: fall through rather than answer nothing
        }
        HoverSite::Global => {
            if let Some(p) = core.params.iter().find(|p| p.name == word) {
                return Some(format!("**{}** — core parameter\n\n{}", p.name, p.doc));
            }
        }
        HoverSite::Other => {}
    }
    hover_markdown_with_core(catalog, core, doc, word)
}

/// [`hover_markdown`] plus core functions, core parameters, and
/// pseudo-variables (`word` may be given without the `$`).
pub fn hover_markdown_with_core(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    word: &str,
) -> Option<String> {
    // A module the config never loads must not shadow the language.
    // Before the module catalogue shipped built in this was
    // unreachable without a source tree; with 186 modules always
    // present, hovering the core global `log_level` resolved to the
    // `opentelemetry` modparam of the same name.
    let loaded = loaded_names(doc);
    if let Some(h) = hover_among(catalog.iter().filter(|m| loaded.contains(&m.name)), word) {
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
    // last: a module the config does not load, and module names
    hover_among(catalog.iter().filter(|m| !loaded.contains(&m.name)), word)
        .or_else(|| hover_module_name(catalog, word))
}
