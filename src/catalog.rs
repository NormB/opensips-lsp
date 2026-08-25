//! Module documentation catalog, harvested from the OpenSIPS docbook
//! sources (`modules/<name>/doc/<name>_admin.xml`).

use std::path::Path;

/// Where a module catalogue came from.
///
/// What a module exports moves between releases, so a diagnostic that
/// does not say which version it judged against cannot be acted on:
/// the reader cannot tell a typo from a parameter their build has and
/// this catalogue does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogOrigin {
    /// The vendored catalogue, at this upstream version.
    BuiltIn(String),
    /// A source tree the user pointed `opensipsSrc` at. It is exact
    /// for their build by construction, so it names no version.
    ConfiguredTree,
}

impl CatalogOrigin {
    /// How to name this catalogue inside a sentence.
    pub fn describe(&self) -> String {
        match self {
            Self::BuiltIn(v) => format!("OpenSIPS {v} (built in)"),
            Self::ConfiguredTree => "the configured source tree".to_string(),
        }
    }
}

/// One documented module symbol: a parameter or a function.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    /// Bare name (`kv_bucket`, `nats_kv_get`).
    pub name: String,
    /// Human detail: the param type (`string`) or function signature.
    pub detail: String,
    /// First documentation paragraph, whitespace-collapsed.
    pub doc: String,
}

/// The harvested documentation of one OpenSIPS module.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleDoc {
    /// Module name (directory name under `modules/`).
    pub name: String,
    /// Exported parameters (`modparam` targets).
    pub params: Vec<Item>,
    /// Exported script functions.
    pub functions: Vec<Item>,
}

/// Harvested documentation is untrusted input rendered as Markdown in
/// editor popups: strip raw HTML and neutralize links whose scheme is
/// not http(s) (`command:`, `javascript:`, ...) — the label survives.
fn sanitize_doc(text: &str) -> String {
    static HTML: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static LINK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let html = HTML.get_or_init(|| regex::Regex::new(r"</?[A-Za-z][^>]*>").unwrap());
    let link = LINK.get_or_init(|| {
        regex::Regex::new(r"\[([^\]]*)\]\(([A-Za-z][A-Za-z0-9+.-]*):[^)]*\)").unwrap()
    });
    let no_html = html.replace_all(text, "");
    link.replace_all(&no_html, |c: &regex::Captures| {
        let scheme = c[2].to_ascii_lowercase();
        if scheme == "http" || scheme == "https" {
            c[0].to_string()
        } else {
            c[1].to_string()
        }
    })
    .into_owned()
}

/// The docbook sources use DTD entities (`&osips;`, `&adminguide;`)
/// whose definitions live in files we do not load; neutralize every
/// non-predefined entity so the XML still parses standalone.
fn neutralize_entities(xml: &str) -> String {
    let re = regex::Regex::new(r"&([a-zA-Z][a-zA-Z0-9._-]*);").unwrap();
    re.replace_all(xml, |c: &regex::Captures| {
        let name = &c[1];
        match name {
            "amp" | "lt" | "gt" | "apos" | "quot" => c[0].to_string(),
            "osips" => "OpenSIPS".to_string(),
            other => other.to_string(),
        }
    })
    .into_owned()
}

fn collapsed_text(node: roxmltree::Node) -> String {
    let mut out = String::new();
    for d in node.descendants().filter(|d| d.is_text()) {
        if let Some(t) = d.text() {
            out.push_str(t);
            out.push(' ');
        }
    }
    sanitize_doc(&out.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// First direct `<para>` child of a section, collapsed.
fn first_para(section: roxmltree::Node) -> String {
    section
        .children()
        .find(|c| c.has_tag_name("para"))
        .map(collapsed_text)
        .unwrap_or_default()
}

/// Parse one `<module>_admin.xml` docbook file.
pub fn parse_admin_xml(module: &str, xml: &str) -> Result<ModuleDoc, String> {
    if xml.contains('\0') {
        return Err("input contains NUL bytes".into());
    }
    if xml.trim().is_empty() {
        return Err("empty input".into());
    }
    let cleaned = neutralize_entities(xml);
    let tree = roxmltree::Document::parse(&cleaned).map_err(|e| e.to_string())?;

    let mut out = ModuleDoc {
        name: module.to_string(),
        ..Default::default()
    };

    for section in tree.descendants().filter(|n| n.has_tag_name("section")) {
        let Some(id) = section.attribute("id") else {
            continue;
        };
        if let Some(pname) = id.strip_prefix("param_") {
            // detail = the "(type)" tail of the title, if present
            let title = section
                .children()
                .find(|c| c.has_tag_name("title"))
                .map(collapsed_text)
                .unwrap_or_default();
            let detail = title
                .rsplit_once('(')
                .and_then(|(_, t)| t.split(')').next())
                .unwrap_or("")
                .trim()
                .to_string();
            out.params.push(Item {
                name: pname.to_string(),
                detail,
                doc: first_para(section),
            });
        } else if let Some(fname) = id.strip_prefix("func_") {
            let signature = section
                .descendants()
                .find(|c| c.has_tag_name("function"))
                .map(collapsed_text)
                .unwrap_or_default();
            out.functions.push(Item {
                name: fname.to_string(),
                detail: signature,
                doc: first_para(section),
            });
        }
    }
    Ok(out)
}

/// Skip ASCII whitespace in place.
fn skip_ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && b[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

/// Strip C comments so a commented-out table entry cannot be read as
/// an export. String and character literals are copied through: a
/// `//` inside a literal is text, not a comment.
fn strip_c_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'') => {
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == q {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.extend_from_slice(&b[start..i.min(b.len())]);
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                // a comment separates tokens; keep them apart
                out.push(b' ');
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The outcome of reading one module's C parameter tables.
pub struct ModuleCParams {
    /// Every `modparam` name found, in declaration order.
    pub names: Vec<String>,
    /// Whether every table resolved fully. A table that splices in a
    /// macro we could not find leaves this false: the name set is
    /// then possibly short, so it must not be used to drop anything.
    pub complete: bool,
}

/// Every `modparam` name declared by the `param_export_t` tables in
/// one C source file, in declaration order and de-duplicated.
///
/// This is the list `modparam()` is checked against when OpenSIPS
/// starts, so it decides which parameters exist; a module README only
/// says what they mean. The type is what makes a table a parameter
/// table — `mi_xmlrpc` names its `mi_params` and wires it into the
/// parameters slot of `struct module_exports` all the same.
pub fn parse_param_export_tables(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut complete = true;
    scan_param_tables(
        src,
        &std::collections::BTreeMap::new(),
        &mut names,
        &mut complete,
    );
    names
}

/// Find every `param_export_t <ident>[] = { ... }` initialiser and
/// collect the names it declares.
fn scan_param_tables(
    src: &str,
    macros: &std::collections::BTreeMap<String, String>,
    out: &mut Vec<String>,
    complete: &mut bool,
) {
    const TY: &str = "param_export_t";
    let stripped = strip_c_comments(src);
    let b = stripped.as_bytes();
    let mut search = 0usize;

    while let Some(rel) = stripped[search..].find(TY) {
        let at = search + rel;
        search = at + TY.len();
        // a whole token, not the tail of some other identifier
        if at > 0 && (b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_') {
            continue;
        }
        // only `<ident>[] = {` opens a table; a prototype or a
        // `param_export_t *` parameter does not
        let mut i = search;
        skip_ws(b, &mut i);
        let id = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
            i += 1;
        }
        if i == id {
            continue;
        }
        let mut shaped = true;
        for expect in *b"[]=" {
            skip_ws(b, &mut i);
            if i >= b.len() || b[i] != expect {
                shaped = false;
                break;
            }
            i += 1;
        }
        if !shaped {
            continue;
        }
        skip_ws(b, &mut i);
        if i >= b.len() || b[i] != b'{' {
            continue;
        }
        search = collect_table_entries(&stripped, i, macros, out, complete, 8);
    }
}

/// Read one table initialiser, `start` at its opening brace, and
/// return the index just past its close.
///
/// Each entry opens a brace one level inside the table and its name is
/// the literal that opens it, so `{0, 0, 0}` terminators contribute
/// nothing and a literal deeper inside an entry's value is an argument
/// rather than a name.
fn collect_table_entries(
    src: &str,
    start: usize,
    macros: &std::collections::BTreeMap<String, String>,
    out: &mut Vec<String>,
    complete: &mut bool,
    budget: u8,
) -> usize {
    let b = src.as_bytes();
    let mut i = start;
    let mut depth = 0usize;

    while i < b.len() {
        match b[i] {
            b'{' => {
                depth += 1;
                if depth == 2 {
                    let mut j = i + 1;
                    skip_ws(b, &mut j);
                    if j < b.len() && b[j] == b'"' {
                        let s = j + 1;
                        let mut e = s;
                        while e < b.len() && b[e] != b'"' {
                            if b[e] == b'\\' {
                                e += 1;
                            }
                            e += 1;
                        }
                        let name = &src[s..e.min(src.len())];
                        if !name.is_empty() && !out.iter().any(|n| n == name) {
                            out.push(name.to_string());
                        }
                    }
                }
                i += 1;
            }
            b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                i += 1;
                if depth == 0 {
                    break;
                }
            }
            b'#' => {
                // A preprocessor directive brackets entries
                // conditionally — `rr` guards `ignore_user` behind
                // `ENABLE_USER_CHECK`. A catalogue wants the union of
                // both arms, so skip the directive rather than
                // reading `ifdef` as a macro this parser cannot find.
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            c if depth == 1 && (c.is_ascii_alphabetic() || c == b'_') => {
                // A bare identifier between entries is a macro that
                // splices in more of them: `registrar` and
                // `mid_registrar` share seven parameters through
                // `reg_modparams` in `lib/reg/common.h`. Reading the
                // table without expanding it reports those seven as
                // absent, and they would then be deleted from the
                // catalogue as phantoms.
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                match macros.get(&src[s..i]) {
                    Some(body) if budget > 0 => {
                        let wrapped = format!("{{{body}}}");
                        collect_table_entries(&wrapped, 0, macros, out, complete, budget - 1);
                    }
                    _ => *complete = false,
                }
            }
            _ => i += 1,
        }
    }
    i
}

/// `#define <name> ...` bodies that look like table entries.
///
/// Only the ones containing a `{"` are kept: those are the ones a
/// parameter table can splice in.
fn collect_entry_macros(
    files: &[std::path::PathBuf],
) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let stripped = strip_c_comments(&text);
        let mut lines = stripped.lines();
        while let Some(line) = lines.next() {
            let Some(rest) = line.trim_start().strip_prefix("#define") else {
                continue;
            };
            let rest = rest.trim_start();
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            if end == 0 {
                continue;
            }
            let name = rest[..end].to_string();
            let mut body = rest[end..].to_string();
            while body.trim_end().ends_with('\\') {
                let Some(next) = lines.next() else { break };
                body.push(' ');
                body.push_str(next);
            }
            let body = body.replace('\\', " ");
            if body.contains("{\"") {
                out.insert(name, body);
            }
        }
    }
    out
}

/// Every C source under `root`, plus its headers when asked.
fn c_sources(root: &Path, headers: bool) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "c")
                || (headers && path.extension().is_some_and(|x| x == "h"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Every `modparam` name a module exports, unioned across every
/// `param_export_t` table under its directory.
///
/// The union is deliberate. Over-collecting is permissive in both
/// directions this catalogue cares about — it drops fewer README
/// entries as phantoms and excuses fewer README examples — whereas
/// under-collecting warns at a configuration that is correct.
pub fn param_names_from_c(module_dir: &Path, tree_root: &Path) -> ModuleCParams {
    let mut macro_files = c_sources(module_dir, true);
    let lib = tree_root.join("lib");
    if lib.is_dir() {
        macro_files.extend(c_sources(&lib, true));
    }
    let macros = collect_entry_macros(&macro_files);

    let mut names = Vec::new();
    let mut complete = true;
    for f in c_sources(module_dir, false) {
        let Ok(src) = std::fs::read_to_string(&f) else {
            continue;
        };
        scan_param_tables(&src, &macros, &mut names, &mut complete);
    }
    ModuleCParams { names, complete }
}

/// Reconcile a README harvest against the module's C parameter tables.
///
/// The C table decides which parameters exist; the README decides what
/// they mean. Two conditions hold a module back to its README harvest
/// untouched: no table at all — `textops` and the other function-only
/// modules export none — and a table this parser could not fully
/// resolve. Either way a parser regression degrades to the previous
/// behaviour rather than deleting real parameters.
fn reconcile_params_with_c(doc: &mut ModuleDoc, module_dir: &Path, tree_root: &Path) {
    let found = param_names_from_c(module_dir, tree_root);
    if found.names.is_empty() {
        return;
    }
    let module = doc.name.clone();
    if found.complete {
        // a heading that misnames a parameter put an entry in the
        // catalogue for something the module never exported
        doc.params.retain(|p| found.names.contains(&p.name));
    }
    for name in found.names {
        if doc.params.iter().any(|p| p.name == name) {
            continue;
        }
        doc.params.push(Item {
            name,
            detail: String::new(),
            doc: format!("Exported by `{module}`; not documented in the module README."),
        });
    }
}

/// Harvest every module's admin docbook under an OpenSIPS source tree.
pub fn harvest_tree(tree_root: &Path) -> Vec<ModuleDoc> {
    let mut out = Vec::new();
    let modules = tree_root.join("modules");
    let Ok(entries) = std::fs::read_dir(&modules) else {
        return out;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        // 4.x markdown is the CURRENT documentation and wins; docbook
        // is the fallback for older trees (and for placeholder
        // READMEs carrying no exported sections).
        let readme = e.path().join("README.md");
        let from_md = std::fs::read_to_string(&readme)
            .ok()
            .and_then(|md| parse_readme_md(&name, &md).ok());
        let picked = match &from_md {
            Some(m) if !m.params.is_empty() || !m.functions.is_empty() => Some(m.clone()),
            _ => {
                let admin = e.path().join("doc").join(format!("{name}_admin.xml"));
                std::fs::read_to_string(&admin)
                    .ok()
                    .and_then(|xml| parse_admin_xml(&name, &xml).ok())
                    // `xml` and `event_datagram` really do export nothing —
                    // documented as `*None*.` — and dropping them left seven
                    // modules of the tree missing from `loadmodule` completion.
                    // An empty harvest is what falls through to docbook; with
                    // no docbook to fall through to, it is the answer.
                    .or_else(|| from_md.clone())
            }
        };
        if let Some(mut m) = picked {
            reconcile_params_with_c(&mut m, &e.path(), tree_root);
            out.push(m);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The parameter names one heading documents.
///
/// `osp` writes `private_key, local_certificate, ca_certificates`
/// under a single heading and `tls_mgm` writes `server_domain,
/// client_domain`; read as one name the entry matched no `modparam`
/// at all.  Only a plain identifier counts: `sp1_uri, sp2_uri, ...,
/// sp16_uri` elides the middle and the elision is not a parameter,
/// and `app[index]_call_state_column` is a template rather than a
/// list, so it is left whole.
fn param_names(heading: &str) -> Vec<String> {
    if !heading.contains(',') {
        return vec![heading.to_string()];
    }
    let names: Vec<String> = heading
        .split(',')
        .map(str::trim)
        .filter(|p| {
            !p.is_empty()
                && p.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(str::to_string)
        .collect();
    if names.is_empty() {
        vec![heading.to_string()]
    } else {
        names
    }
}

/// Parse a 4.x-style `modules/<name>/README.md` (markdown docs).
///
/// Recognizes `### Exported Parameters` / `### Exported Functions`
/// sections with one `#### <entry>` heading per item; the first
/// paragraph after the heading is the doc summary.
pub fn parse_readme_md(module: &str, md: &str) -> Result<ModuleDoc, String> {
    if md.contains('\0') {
        return Err("input contains NUL bytes".into());
    }
    if md.trim().is_empty() {
        return Err("empty input".into());
    }

    #[derive(PartialEq, Clone, Copy)]
    enum Section {
        Params,
        Functions,
        Other,
    }
    /// The section a heading opens, whatever depth it is written at.
    fn section_of(heading: &str) -> Option<Section> {
        match heading {
            "Exported Parameters" => Some(Section::Params),
            "Exported Functions" => Some(Section::Functions),
            _ => None,
        }
    }
    let mut section = Section::Other;
    let mut out = ModuleDoc {
        name: module.to_string(),
        ..Default::default()
    };
    /// The heading being read, and the prose accumulating under it.
    /// One heading can document several parameters, and they share
    /// everything but the name.
    struct Pending {
        is_param: bool,
        names: Vec<String>,
        detail: String,
        lines: Vec<String>,
        /// the first paragraph is the summary; the rest is skipped
        finished: bool,
    }
    let mut cur: Option<Pending> = None;

    let flush = |cur: &mut Option<Pending>, out: &mut ModuleDoc| {
        if let Some(p) = cur.take() {
            let doc = sanitize_doc(
                &p.lines
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            for name in p.names {
                let item = Item {
                    name,
                    detail: p.detail.clone(),
                    doc: doc.clone(),
                };
                if p.is_param {
                    out.params.push(item);
                } else {
                    out.functions.push(item);
                }
            }
        }
    };

    let mut in_fence = false;
    for line in md.lines() {
        // Every parameter in a real README is followed by a fenced
        // `modparam` example, and those examples carry `#` comments.
        // Read as headings they closed the section, and everything
        // documented below the first example was thrown away.
        let start = line.trim_start();
        if start.starts_with("```") || start.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(h3) = line.strip_prefix("### ") {
            flush(&mut cur, &mut out);
            section = section_of(h3.trim()).unwrap_or(Section::Other);
            continue;
        }
        if let Some(h4) = line.strip_prefix("#### ") {
            flush(&mut cur, &mut out);
            let heading = h4.trim();
            // three cachedb modules nest `#### Exported Functions`
            // inside the parameters chapter rather than beside it.
            // Read as an item it became a PARAMETER called `Exported
            // Functions`; it is a section wherever it is written.
            if let Some(s) = section_of(heading) {
                section = s;
                continue;
            }
            match section {
                Section::Params => {
                    // `fr_timeout (integer)` and `db_url(str)` are the
                    // same heading written two ways; the name is what a
                    // `modparam` would write, which is everything before
                    // the type, space or no space
                    let (name, detail) = match heading.split_once('(') {
                        Some((n, rest)) => (
                            n.trim(),
                            rest.trim_end().trim_end_matches(')').trim().to_string(),
                        ),
                        None => (heading, String::new()),
                    };
                    cur = Some(Pending {
                        is_param: true,
                        names: param_names(name),
                        detail,
                        lines: Vec::new(),
                        finished: false,
                    });
                }
                Section::Functions => {
                    let name = heading
                        .split('(')
                        .next()
                        .unwrap_or(heading)
                        .trim()
                        .to_string();
                    cur = Some(Pending {
                        is_param: false,
                        names: vec![name],
                        detail: heading.to_string(),
                        lines: Vec::new(),
                        finished: false,
                    });
                }
                Section::Other => {}
            }
            continue;
        }
        if line.starts_with("#####") {
            // deeper than an item: a sub-heading inside one entry's
            // prose (`##### Authentication` under `cachedb_url`), so
            // the entry — and the section — continue past it
            continue;
        }
        if line.starts_with('#') {
            // any other heading level ends the current item
            flush(&mut cur, &mut out);
            section = Section::Other;
            continue;
        }
        if let Some(p) = cur.as_mut() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !p.lines.is_empty() {
                    p.finished = true; // first paragraph complete
                }
            } else if !p.finished {
                p.lines.push(trimmed.to_string());
            }
        }
    }
    flush(&mut cur, &mut out);
    Ok(out)
}

/// Shared heading-walker for the 4.x manual markdown pages: collects
/// `(heading, first paragraph)` pairs for headings at `level`.
fn md_sections(md: &str, level: usize) -> Result<Vec<(String, String)>, String> {
    if md.contains('\0') {
        return Err("input contains NUL bytes".into());
    }
    if md.trim().is_empty() {
        return Err("empty input".into());
    }
    let marker = format!("{} ", "#".repeat(level));
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, Vec<String>, bool)> = None;
    let mut in_fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(h) = line.strip_prefix(&marker) {
            if let Some((name, lines, _)) = cur.take() {
                out.push((name, sanitize_doc(&lines.join(" "))));
            }
            cur = Some((h.trim().to_string(), Vec::new(), false));
            continue;
        }
        if line.starts_with('#') {
            if let Some((name, lines, _)) = cur.take() {
                out.push((name, sanitize_doc(&lines.join(" "))));
            }
            continue;
        }
        if let Some((_, lines, finished)) = cur.as_mut() {
            let t = line.trim();
            if t.is_empty() {
                if !lines.is_empty() {
                    *finished = true;
                }
            } else if !*finished {
                lines.push(t.to_string());
            }
        }
    }
    if let Some((name, lines, _)) = cur.take() {
        out.push((name, sanitize_doc(&lines.join(" "))));
    }
    Ok(out)
}

/// Parse `docs/manual/Script-CoreFunctions.md` (4.x): one `##
/// name(sig)` heading per function, first paragraph = doc.
pub fn parse_core_functions_md(md: &str) -> Result<Vec<Item>, String> {
    Ok(md_sections(md, 2)?
        .into_iter()
        .filter_map(|(heading, doc)| {
            let name = heading.split('(').next()?.trim();
            if name.is_empty() || !heading.contains('(') {
                return None;
            }
            Some(Item {
                name: name.to_string(),
                detail: heading,
                doc,
            })
        })
        .collect())
}

/// Parse `docs/manual/Script-CoreParameters.md` (4.x): `### name`
/// headings, first paragraph = doc.
pub fn parse_core_params_md(md: &str) -> Result<Vec<Item>, String> {
    Ok(md_sections(md, 3)?
        .into_iter()
        .filter_map(|(heading, doc)| {
            let name = heading.trim();
            if name.is_empty() || name.contains(' ') {
                return None;
            }
            Some(Item {
                name: name.to_string(),
                detail: "core parameter".into(),
                doc,
            })
        })
        .collect())
}

/// Parse `docs/manual/Script-CoreVar.md` (4.x): `### Description -
/// $name` headings, first paragraph = doc.
pub fn parse_core_vars_md(md: &str) -> Result<Vec<Item>, String> {
    Ok(md_sections(md, 3)?
        .into_iter()
        .filter_map(|(heading, doc)| {
            let (desc, var) = heading.rsplit_once(" - $")?;
            let var = var.trim();
            if var.is_empty() {
                return None;
            }
            Some(Item {
                name: format!("${var}"),
                detail: desc.trim().to_string(),
                doc,
            })
        })
        .collect())
}

/// Core-language documentation harvested from `docs/manual/` (4.x).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CoreDocs {
    /// Core script functions (`Script-CoreFunctions.md`).
    pub functions: Vec<Item>,
    /// Global core parameters (`Script-CoreParameters.md`).
    pub params: Vec<Item>,
    /// Pseudo-variables (`Script-CoreVar.md`), names include the `$`.
    pub pvars: Vec<Item>,
}

/// The vendored core catalogue: what the core language looks like in
/// the version this release pins.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuiltinCore {
    /// The OpenSIPS version the docs were harvested from.
    pub version: String,
    /// The harvested core docs.
    pub core: CoreDocs,
}

/// The built-in core catalogue, used when no source tree is
/// configured.
///
/// Core parameters, functions and pseudo-variables are the LANGUAGE,
/// not a module: requiring a full source checkout before `log_level`
/// completes makes the extension useless out of the box.  A tree the
/// user configures still wins, because only that is exact for their
/// build — so every built-in entry says which version it came from.
pub fn builtin_core() -> &'static BuiltinCore {
    static B: std::sync::OnceLock<BuiltinCore> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let mut b: BuiltinCore = serde_json::from_str(include_str!("core_builtin.json"))
            .expect("the vendored core catalogue must parse");
        let note = format!(
            "\n\n*Built-in documentation from OpenSIPS {} — set `opensipsSrc` \
             to your own source tree for version-exact docs.*",
            b.version
        );
        for it in b
            .core
            .functions
            .iter_mut()
            .chain(b.core.params.iter_mut())
            .chain(b.core.pvars.iter_mut())
        {
            it.doc.push_str(&note);
        }
        b
    })
}

/// The vendored module catalogue: every module the pinned release
/// documents, with its exported functions and parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuiltinModules {
    /// The OpenSIPS version the docs were harvested from.
    pub version: String,
    /// One entry per documented module.
    pub modules: Vec<ModuleDoc>,
}

/// The built-in module catalogue, used when no source tree is
/// configured.
///
/// `is_method` is a `sipmsgops` function, not core, so the core
/// catalogue alone still left every module call undocumented and
/// `loadmodule "` offering nothing at all.  What a module exports does
/// move between releases — which is exactly why a configured tree
/// REPLACES this wholesale rather than merging with it: two versions
/// blended together would be wrong in a way neither is on its own.
/// Every built-in entry says which version it came from.
pub fn builtin_modules() -> &'static BuiltinModules {
    static B: std::sync::OnceLock<BuiltinModules> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let mut b: BuiltinModules = serde_json::from_str(include_str!("modules_builtin.json"))
            .expect("the vendored module catalogue must parse");
        let note = format!(
            "\n\n*Built-in documentation from OpenSIPS {} — set `opensipsSrc` \
             to your own source tree for version-exact docs.*",
            b.version
        );
        for m in b.modules.iter_mut() {
            for it in m.functions.iter_mut().chain(m.params.iter_mut()) {
                it.doc.push_str(&note);
            }
        }
        b
    })
}

/// Harvest the core-language docs from an OpenSIPS 4.x source tree;
/// missing or unparsable pages simply yield empty sections.
pub fn harvest_core(tree_root: &Path) -> CoreDocs {
    let manual = tree_root.join("docs").join("manual");
    let read = |f: &str| std::fs::read_to_string(manual.join(f)).unwrap_or_default();
    CoreDocs {
        functions: parse_core_functions_md(&read("Script-CoreFunctions.md")).unwrap_or_default(),
        params: parse_core_params_md(&read("Script-CoreParameters.md")).unwrap_or_default(),
        pvars: parse_core_vars_md(&read("Script-CoreVar.md")).unwrap_or_default(),
    }
}

/// Cache format version: bump when `CacheFile` or the harvest
/// semantics change, so caches written by older builds self-miss.
const CACHE_SCHEMA_VERSION: u32 = 2;

/// A content-aware change-detector for a source tree: canonical path,
/// schema version, and a manifest of every file the harvest reads —
/// `modules/*/README.md`, `modules/*/doc/*_admin.xml`, and the core
/// manual pages — as (path, size, mtime).  In-place edits, additions,
/// and removals of harvested files all change the fingerprint;
/// directory mtimes are not consulted.
pub fn tree_fingerprint(tree_root: &Path) -> String {
    use std::fmt::Write;
    use std::time::UNIX_EPOCH;
    let stat = |p: &Path| -> Option<(u64, u128)> {
        let m = std::fs::metadata(p).ok()?;
        if !m.is_file() {
            return None;
        }
        let t = m
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos();
        Some((m.len(), t))
    };
    let canon = std::fs::canonicalize(tree_root).unwrap_or_else(|_| tree_root.to_path_buf());
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(canon.join("modules")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            files.push(e.path().join("README.md"));
            files.push(e.path().join("doc").join(format!("{name}_admin.xml")));
        }
    }
    let manual = canon.join("docs").join("manual");
    for f in [
        "Script-CoreFunctions.md",
        "Script-CoreParameters.md",
        "Script-CoreVar.md",
    ] {
        files.push(manual.join(f));
    }
    files.sort();
    let mut raw = format!("v{CACHE_SCHEMA_VERSION}|{}", canon.display());
    for f in files {
        if let Some((len, mt)) = stat(&f) {
            let _ = write!(raw, "|{}:{len}:{mt}", f.display());
        }
    }
    // stable, filesystem-safe name
    let mut h: u64 = 0xcbf29ce484222325;
    for b in raw.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheFile {
    modules: Vec<ModuleDoc>,
    core: CoreDocs,
}

/// Load a cached harvest for `tree_root`, if the cache is present,
/// parseable, and matches the tree's current fingerprint.
pub fn load_cached(tree_root: &Path, cache_dir: &Path) -> Option<(Vec<ModuleDoc>, CoreDocs)> {
    let f = cache_dir.join(format!("{}.json", tree_fingerprint(tree_root)));
    let bytes = std::fs::read(f).ok()?;
    let c: CacheFile = serde_json::from_slice(&bytes).ok()?;
    Some((c.modules, c.core))
}

/// Persist a harvest under the tree's current fingerprint.
pub fn save_cache(
    tree_root: &Path,
    cache_dir: &Path,
    modules: &[ModuleDoc],
    core: &CoreDocs,
) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    let fp = tree_fingerprint(tree_root);
    let f = cache_dir.join(format!("{fp}.json"));
    let c = CacheFile {
        modules: modules.to_vec(),
        core: core.clone(),
    };
    let bytes = serde_json::to_vec(&c).map_err(|e| e.to_string())?;
    // atomic publish: concurrent servers may write the same file, and
    // readers must never see a torn cache — write-then-rename
    let tmp = cache_dir.join(format!(".{fp}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &f).map_err(|e| e.to_string())
}
