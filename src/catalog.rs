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
        .collect::<Vec<_>>())
    .map(|mut v: Vec<Item>| {
        dedup_by_name(&mut v);
        v
    })
}

/// Parse `docs/manual/Script-Statements.md` (4.x): `## name`
/// headings, first paragraph = doc.
///
/// Two things the page does that a verbatim reading gets wrong.
///
/// The iteration statement is headed `for each`, which is prose: the
/// keyword a configuration writes is `for`, and documenting `for each`
/// would answer for something nobody can type.
///
/// And several keywords have no section of their own — `else` is
/// explained under `if`, `case` and `default` under `switch`.
/// Answering nothing for them leaves the commonest keywords in the
/// language silent; inventing text for them would be worse. They
/// carry the explaining section's text and say which section that is.
/// Each is named here rather than guessed from proximity.
pub fn parse_core_statements_md(md: &str) -> Result<Vec<Item>, String> {
    /// Headings whose text is not the keyword.
    const HEADING_IS_PROSE: &[(&str, &str)] = &[("for each", "for")];
    /// Keywords explained inside another statement's section.
    const EXPLAINED_UNDER: &[(&str, &str)] =
        &[("else", "if"), ("case", "switch"), ("default", "switch")];

    let mut out: Vec<Item> = md_sections(md, 2)?
        .into_iter()
        .filter_map(|(heading, doc)| {
            let heading = heading.trim();
            if doc.trim().is_empty() {
                return None;
            }
            let name = match HEADING_IS_PROSE
                .iter()
                .find(|(h, _)| h.eq_ignore_ascii_case(heading))
            {
                Some((_, kw)) => (*kw).to_string(),
                None => heading_keyword(heading, &doc)?,
            };
            Some(Item {
                name,
                detail: "control statement".to_string(),
                doc,
            })
        })
        .collect();

    dedup_by_name(&mut out);
    for (alias, parent) in EXPLAINED_UNDER {
        let Some(owner) = out.iter().find(|s| &s.name == parent) else {
            continue;
        };
        let doc = owner.doc.clone();
        out.push(Item {
            name: (*alias).to_string(),
            detail: format!("control statement — documented under `{parent}`"),
            doc,
        });
    }
    Ok(out)
}

/// Fold a `## heading` into the keyword it names, or `None`.
///
/// Two things a straight read gets wrong, both one line away from
/// being true of these pages.
///
/// A heading may be capitalised — `## If` — and a name differing by
/// case never matches a hover, so the keyword silently disappears
/// while the parser reports success. Keywords here are lower case, so
/// the heading is folded.
///
/// A section may carry no prose at all. An entry with empty text
/// hovers as a header with nothing under it, which reads as the
/// server being broken rather than the page being thin. It is not
/// documentation, so it is not offered.
fn heading_keyword(heading: &str, doc: &str) -> Option<String> {
    if doc.trim().is_empty() {
        return None;
    }
    let name = heading.trim().to_ascii_lowercase();
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then_some(name)
}

/// Keep the first entry of each name.
///
/// A page with two `## if` sections yields two entries. Every lookup
/// is a `find`, so the second is dead weight — except in completion,
/// which offers both.
fn dedup_by_name(items: &mut Vec<Item>) {
    let mut seen: Vec<String> = Vec::new();
    items.retain(|i| {
        if seen.contains(&i.name) {
            return false;
        }
        seen.push(i.name.clone());
        true
    });
}

/// Parse `docs/manual/Script-Async.md` (4.x): the two statements it
/// documents, and nothing else on the page.
///
/// The headings are prose — `## async() statement` — and they sit
/// beside `## Description` and `## Limitations`, which are
/// identifier-shaped and are not keywords. A parser loose enough to
/// take the first pair takes the second pair too, and then offers
/// `Description` as something a configuration can write. So the
/// headings that mean something are named here, and nothing else is
/// taken.
pub fn parse_core_async_md(md: &str) -> Result<Vec<Item>, String> {
    /// Heading text -> the keyword it documents.
    const STATEMENTS: &[(&str, &str)] = &[
        ("async() statement", "async"),
        ("launch() statement", "launch"),
    ];
    Ok(md_sections(md, 2)?
        .into_iter()
        .filter_map(|(heading, doc)| {
            let name = STATEMENTS
                .iter()
                .find(|(h, _)| *h == heading.trim())
                .map(|(_, kw)| *kw)?;
            Some(Item {
                name: name.to_string(),
                detail: "control statement".to_string(),
                doc,
            })
        })
        .collect())
}

/// Parse `docs/manual/Script-Routes.md` (4.x): `## name` headings,
/// first paragraph = doc.
///
/// Unlike the core functions page, a heading here is a bare keyword
/// with no argument list, so anything parenthesised is prose rather
/// than a route type.
pub fn parse_core_routes_md(md: &str) -> Result<Vec<Item>, String> {
    let mut out: Vec<Item> = md_sections(md, 2)?
        .into_iter()
        .filter_map(|(heading, doc)| {
            let name = heading_keyword(&heading, &doc)?;
            Some(Item {
                name,
                detail: "route type".to_string(),
                doc,
            })
        })
        .collect();
    dedup_by_name(&mut out);
    Ok(out)
}

/// Parse `docs/manual/Script-CoreParameters.md` (4.x): `### name`
/// headings, first paragraph = doc.
/// The raw body of every section at `level`, fenced blocks and all.
///
/// `md_sections` summarises: it takes the first paragraph and skips
/// fenced blocks, which is right for a route type or a control
/// statement where the summary IS the answer. For a setting the
/// answer is what to write, and that lives in the example.
fn md_sections_raw(md: &str, level: usize) -> Result<Vec<(String, String)>, String> {
    if md.contains('\0') {
        return Err("input contains NUL bytes".into());
    }
    if md.trim().is_empty() {
        return Err("empty input".into());
    }
    let marker = format!("{} ", "#".repeat(level));
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, Vec<&str>)> = None;
    let mut in_fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            // a heading only ends a section outside a fenced block:
            // `### ` inside an example is example text
            if let Some(h) = line.strip_prefix(&marker) {
                if let Some((name, body)) = cur.take() {
                    out.push((name, body.join("\n")));
                }
                cur = Some((h.trim().to_string(), Vec::new()));
                continue;
            }
            if line.starts_with('#') {
                if let Some((name, body)) = cur.take() {
                    out.push((name, body.join("\n")));
                }
                continue;
            }
        }
        if let Some((_, body)) = cur.as_mut() {
            body.push(line);
        }
    }
    if let Some((name, body)) = cur.take() {
        out.push((name, body.join("\n")));
    }
    Ok(out)
}

/// A setting's documentation: what it is, what it defaults to, and
/// what writing it looks like.
///
/// `db_default_url` is the case that named this. Its description says
/// it is "the default DB URL used by modules"; the URL FORMAT — the
/// only thing a reader is hovering it to find — is in the example
/// block below that. Keeping the description alone answered the
/// question nobody asked.
fn setting_doc(body: &str) -> String {
    let mut description: Vec<&str> = Vec::new();
    let mut default: Option<String> = None;
    let mut example: Vec<&str> = Vec::new();
    let mut in_fence = false;
    let mut fence_done = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            // the FIRST fenced block is the worked example; a section
            // with several is showing variations, and a hover wants one
            if in_fence {
                fence_done = true;
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            if !fence_done {
                example.push(line.trim_end());
            }
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("Default value is") {
            let rest = rest.trim().trim_end_matches('.').trim();
            if !rest.is_empty() {
                default = Some(rest.to_string());
            }
            continue;
        }
        if t.starts_with("Example of usage") {
            continue;
        }
        if default.is_none() && example.is_empty() {
            description.push(t);
        }
    }
    // a markdown list must survive as a list: `socket` documents its
    // twelve optional modifiers one bullet each, and joining those
    // with spaces gives one unbroken paragraph of twelve settings
    let mut joined = String::new();
    for line in &description {
        let is_item = line.starts_with("* ") || line.starts_with("- ");
        if joined.is_empty() {
            joined.push_str(line);
        } else if is_item {
            joined.push('\n');
            joined.push_str(line);
        } else {
            joined.push(' ');
            joined.push_str(line);
        }
    }
    let mut out = sanitize_doc(&joined);
    if let Some(d) = default {
        out.push_str(&format!("\n\nDefault: {d}."));
    }
    let example: Vec<&str> = example
        .iter()
        .skip_while(|l| l.trim().is_empty())
        .copied()
        .collect();
    let example = example.join("\n");
    if !example.trim().is_empty() {
        out.push_str(&format!("\n\n```opensips\n{}\n```", example.trim_end()));
    }
    out
}

/// Parse `docs/manual/Script-CoreParameters.md` (4.x): `### name`
/// headings, each carrying what the setting is, what it defaults to,
/// and what writing it looks like.
pub fn parse_core_params_md(md: &str) -> Result<Vec<Item>, String> {
    Ok(md_sections_raw(md, 3)?
        .into_iter()
        .filter_map(|(heading, body)| {
            let name = heading.trim();
            if name.is_empty() || name.contains(' ') {
                return None;
            }
            let doc = setting_doc(&body);
            if doc.trim().is_empty() {
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

/// The modifiers `socket = ...` accepts, from the grammar production
/// that parses them, spelled as the lexer spells them.
///
/// Membership is the grammar's, not the manual's — the same rule that
/// makes `param_export_t` the authority for module parameters. Only
/// `socket =` uses this production, which is why hovering `as` or
/// `tag` anywhere else must stay silent: they are ordinary words.
pub fn parse_socket_modifiers_c(cfg_y: &str, cfg_lex: &str) -> Vec<String> {
    let Some(body) = cfg_y
        .split_once("socket_def_param:")
        .and_then(|(_, r)| r.split_once("socket_def_params:"))
        .map(|(b, _)| b)
    else {
        return Vec::new();
    };
    // how the lexer spells each token: the first lower-case
    // alternative, or a bare one-word definition (`TAG   tag`)
    // A spelling may be written in PARTS:
    // `("allow"|"ALLOW")[-_]("proxy"|"PROXY")([-_]("protocol"|"PROTOCOL"))?`
    // is `allow_proxy_protocol`, not `allow`. Take the first
    // lower-case alternative of each group, in order, and join them
    // the way the pattern joins them.
    static GROUP: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let group = GROUP.get_or_init(|| regex::Regex::new(r"\(([^()]*)\)").unwrap());
    let lower = |t: &str| -> Option<String> {
        t.split('"')
            .find(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .map(str::to_string)
    };
    let mut spelling: Vec<(String, String)> = Vec::new();
    for line in cfg_lex.split("\n%%").next().unwrap_or(cfg_lex).lines() {
        let mut it = line.splitn(2, [' ', '\t']);
        let (Some(tok), Some(pat)) = (it.next(), it.next()) else {
            continue;
        };
        if tok.is_empty() || !tok.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            continue;
        }
        let pat = pat.trim();
        let parts: Vec<String> = group
            .captures_iter(pat)
            .filter_map(|c| lower(&c[1]))
            .collect();
        let word = if parts.is_empty() {
            (!pat.is_empty() && pat.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
                .then(|| pat.to_string())
        } else {
            Some(parts.join("_"))
        };
        if let Some(w) = word {
            spelling.push((tok.to_string(), w));
        }
    }
    let mut out: Vec<String> = Vec::new();
    for tok in body
        .split('|')
        .filter_map(|alt| alt.split_whitespace().next())
        .filter(|t| t.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
    {
        if let Some((_, w)) = spelling.iter().find(|(t, _)| t == tok)
            && !out.contains(w)
        {
            out.push(w.clone());
        }
    }
    out
}

/// The manual's descriptions of those modifiers: the bullet list in
/// the `socket` section, one bullet per modifier.
pub fn parse_socket_modifiers_md(md: &str) -> Vec<Item> {
    let Some(section) = md_sections_raw(md, 3)
        .ok()
        .and_then(|v| v.into_iter().find(|(h, _)| h.trim() == "socket"))
        .map(|(_, b)| b)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in section.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("* `").or_else(|| t.strip_prefix("- `")) else {
            continue;
        };
        let Some((form, desc)) = rest.split_once('`') else {
            continue;
        };
        let Some(name) = form.split_whitespace().next() else {
            continue;
        };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let desc = desc.trim_start_matches(':').trim();
        out.push(Item {
            name: name.to_string(),
            detail: format!("socket modifier — `{form}`"),
            doc: sanitize_doc(desc),
        });
    }
    out
}

/// The modifiers a tree accepts, described by its own manual.
///
/// The grammar decides membership; the manual supplies the text. A
/// modifier the grammar accepts and the manual does not describe is
/// still offered — saying so is more use than pretending it does not
/// exist.
pub fn harvest_socket_modifiers(tree_root: &Path) -> Vec<Item> {
    let read = |p: &str| std::fs::read_to_string(tree_root.join(p)).unwrap_or_default();
    let accepted = parse_socket_modifiers_c(&read("cfg.y"), &read("cfg.lex"));
    let documented = parse_socket_modifiers_md(&read("docs/manual/Script-CoreParameters.md"));
    accepted
        .into_iter()
        .map(|name| {
            documented
                .iter()
                .find(|d| d.name == name)
                .cloned()
                .unwrap_or_else(|| Item {
                    name: name.clone(),
                    detail: "socket modifier".into(),
                    doc: format!(
                        "The grammar accepts `{name}` on a `socket` line. The manual \
                         does not describe it."
                    ),
                })
        })
        .collect()
}

/// The log levels `xlog` accepts, read from the C `switch` that
/// parses them.
///
/// The set is not documentation anywhere a harvester can read it, it
/// differs between releases, and it differs between the two servers —
/// so it comes from the source, like `param_export_t` does for module
/// parameters. The switch dispatches on the THIRD character of the
/// string (`s.s[2]`), so a `case` letter that is not the third
/// character of the level it assigns means the shape has changed and
/// the pairing cannot be trusted; such a case is dropped rather than
/// guessed at.
///
/// Anchored on the `unknown log level` message in the default arm: it
/// is what distinguishes this switch from every other `case 'X':` in
/// a source tree.
pub fn parse_log_levels_c(src: &str) -> Vec<String> {
    static CASE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let case = CASE.get_or_init(|| {
        regex::Regex::new(r"case\s*'([A-Z])'\s*:[^;{}]*?(L_[A-Z][A-Z0-9_]*)").unwrap()
    });
    let Some(end) = src.find("unknown log level") else {
        return Vec::new();
    };
    // the switch body precedes its default arm; bound the window so a
    // switch far above the message cannot bleed into it
    let start = src[..end].rfind("switch").unwrap_or(0);
    let mut out: Vec<String> = Vec::new();
    for c in case.captures_iter(&src[start..end]) {
        let letter = c[1].chars().next().unwrap_or(' ');
        // `L_CRIT2` is the internal constant for the level a script
        // spells `L_CRIT`; the trailing digit is not part of the name
        let name = c[2].trim_end_matches(|ch: char| ch.is_ascii_digit());
        if name.chars().nth(2) != Some(letter) {
            continue;
        }
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// An `##` section of `Script-CoreVar.md` opening with a `**Naming**:`
/// line documents a variable KIND — `$var`, `$avp` — rather than one
/// named variable. The exception is a section that has `###` entries
/// of its own: there the naming line is the placeholder those entries
/// share (`## Reference Variables` names `$name`), so the section is a
/// category and reading it as an entry would invent a `$name` that no
/// configuration can write.
fn parse_core_var_kinds(md: &str) -> Vec<Item> {
    /// Heading, the text of its `**Naming**:` line, its first
    /// paragraph after that line, and whether `###` entries follow.
    struct Section {
        naming: Option<String>,
        prose: Vec<String>,
        prose_done: bool,
        has_entries: bool,
    }
    let mut out = Vec::new();
    let mut cur: Option<Section> = None;
    let mut in_fence = false;
    let finish = |cur: &mut Option<Section>, out: &mut Vec<Item>| {
        let Some(sec) = cur.take() else { return };
        let (Some(naming), false) = (sec.naming, sec.has_entries) else {
            return;
        };
        let detail = naming.replace('`', "");
        let Some(rest) = detail.split_once('$').map(|(_, r)| r) else {
            return;
        };
        let var: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if var.is_empty() {
            return;
        }
        out.push(Item {
            name: format!("${var}"),
            detail: detail.trim().to_string(),
            doc: sanitize_doc(&sec.prose.join(" ")),
        });
    };
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if line.starts_with("## ") {
            finish(&mut cur, &mut out);
            cur = Some(Section {
                naming: None,
                prose: Vec::new(),
                prose_done: false,
                has_entries: false,
            });
            continue;
        }
        if line.starts_with("###") {
            if let Some(sec) = cur.as_mut() {
                sec.has_entries = true;
            }
            continue;
        }
        if line.starts_with('#') {
            finish(&mut cur, &mut out);
            continue;
        }
        let Some(sec) = cur.as_mut() else { continue };
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("**Naming**:") {
            // the naming line is the syntax, not the description: the
            // paragraph after it is what a hover should say
            sec.naming = Some(rest.trim().to_string());
        } else if t.is_empty() {
            if !sec.prose.is_empty() {
                sec.prose_done = true;
            }
        } else if sec.naming.is_some() && !sec.prose_done {
            sec.prose.push(t.to_string());
        }
    }
    finish(&mut cur, &mut out);
    out
}

/// Parse `docs/manual/Script-CoreVar.md` (4.x): `### Description -
/// $name` headings, first paragraph = doc, plus the `##` sections
/// that document a whole variable kind.
pub fn parse_core_vars_md(md: &str) -> Result<Vec<Item>, String> {
    let mut out: Vec<Item> = md_sections(md, 3)?
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
        .collect();
    // a named `###` entry is the more specific documentation, so it
    // wins over a kind section spelling the same name
    out.extend(parse_core_var_kinds(md));
    dedup_by_name(&mut out);
    Ok(out)
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
    /// Control statements (`Script-Statements.md`): `if`, `switch`,
    /// `while`, `for`, plus the keywords those sections explain.
    #[serde(default)]
    pub statements: Vec<Item>,
    /// Route types (`Script-Routes.md`): the blocks a configuration is
    /// built out of. Read from the page beside the others, which the
    /// harvester passed over — so hovering `startup_route` answered
    /// nothing at all.
    #[serde(default)]
    pub routes: Vec<Item>,
    /// The modifiers a `socket = ...` line accepts after its
    /// address. Membership from the grammar, text from the manual.
    #[serde(default)]
    pub socket_modifiers: Vec<Item>,
    /// The log levels `xlog`'s first argument accepts, in the order
    /// the C switch lists them. Read from the source, not from prose.
    #[serde(default)]
    pub log_levels: Vec<String>,
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
        let b: BuiltinCore = serde_json::from_str(include_str!("core_builtin.json"))
            .expect("the vendored core catalogue must parse");
        b
    })
}

/// What one release changed from the release before it.
///
/// Adds and updates share `upserted` deliberately: applying either is
/// the same operation — replace the entry of that name, or insert it
/// if absent — and whether a given upsert is an addition or an edit is
/// a question about the previous release, not a fact worth storing
/// twice.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleDelta {
    /// The release this delta produces.
    pub version: String,
    /// Modules this release introduces, in full.
    pub modules_added: Vec<ModuleDoc>,
    /// Modules this release drops.
    pub modules_removed: Vec<String>,
    /// What changed inside modules present in both releases.
    pub changes: Vec<ModuleChange>,
}

/// How one surface — a module's parameters, or its functions —
/// changed between two releases.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SurfaceChange {
    /// Item-level edits, keyed by name.
    Edits {
        /// Names the release no longer exports.
        removed: Vec<String>,
        /// Entries added or altered, applied by name.
        upserted: Vec<Item>,
    },
    /// The whole list, replacing what was there.
    ///
    /// Names are not unique on every surface: `auth` documents
    /// `append_rpid_hf()` and `append_rpid_hf(prefix, suffix)` as two
    /// entries sharing a name, and keying by name would silently
    /// merge the overloads into one. Four surfaces per release need
    /// this; everything else stays item-level, which is why the
    /// deltas are a third of what whole lists would cost.
    Whole(Vec<Item>),
}

/// What one release changed inside one surviving module.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleChange {
    /// The module these changes apply to.
    pub module: String,
    /// How its parameters changed, if they did.
    pub params: Option<SurfaceChange>,
    /// How its functions changed, if they did.
    pub functions: Option<SurfaceChange>,
}

/// The vendored catalogue: one release in full, plus a forward delta
/// per later release.
///
/// Releases differ far less than they resemble each other — between
/// 3.6.8 and 4.0.1, 1660 parameters are identical and 30 are not — so
/// shipping each release whole would be mostly duplicated bytes, and
/// the duplication would grow with every release added rather than
/// shrink.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VersionedModules {
    /// The oldest supported release, in full.
    pub base: BuiltinModules,
    /// Later releases, oldest first, each relative to the one before.
    pub deltas: Vec<ModuleDelta>,
}

/// Put a catalogue in canonical order: modules by name, and each
/// module's parameters and functions by name.
///
/// Order is not information here. Left alone it would be treated as
/// information by the delta: a README that merely reshuffled its
/// headings would produce a delta rewriting every entry, and a
/// reconstructed release would differ from a fresh harvest over
/// nothing. Canonical order makes the round-trip exact and keeps a
/// delta to what actually changed.
pub fn canonicalize(modules: &mut [ModuleDoc]) {
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    for m in modules.iter_mut() {
        m.params.sort_by(|a, b| a.name.cmp(&b.name));
        m.functions.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

/// Whether a surface documents one name more than once.
fn has_duplicate_names(items: &[Item]) -> bool {
    let mut seen: Vec<&str> = Vec::with_capacity(items.len());
    for it in items {
        if seen.contains(&it.name.as_str()) {
            return true;
        }
        seen.push(&it.name);
    }
    false
}

/// Apply one item list over another by name: replace what is there,
/// append what is not.
fn upsert_items(into: &mut Vec<Item>, items: &[Item]) {
    for it in items {
        match into.iter_mut().find(|x| x.name == it.name) {
            Some(slot) => *slot = it.clone(),
            None => into.push(it.clone()),
        }
    }
}

/// How `new` differs from `old`, or `None` if it does not.
fn diff_surface(old: &[Item], new: &[Item]) -> Option<SurfaceChange> {
    if old == new {
        return None;
    }
    if has_duplicate_names(old) || has_duplicate_names(new) {
        return Some(SurfaceChange::Whole(new.to_vec()));
    }
    let removed = old
        .iter()
        .filter(|o| !new.iter().any(|n| n.name == o.name))
        .map(|o| o.name.clone())
        .collect();
    let upserted = new
        .iter()
        .filter(|n| old.iter().find(|o| o.name == n.name) != Some(n))
        .cloned()
        .collect();
    Some(SurfaceChange::Edits { removed, upserted })
}

/// Apply one surface change in place.
fn apply_surface(into: &mut Vec<Item>, change: &SurfaceChange) {
    match change {
        SurfaceChange::Whole(items) => *into = items.clone(),
        SurfaceChange::Edits { removed, upserted } => {
            into.retain(|i| !removed.contains(&i.name));
            upsert_items(into, upserted);
        }
    }
}

/// Compute what `newer` changed from `older`.
///
/// Both must already be in canonical order. Kept beside the type it
/// produces so the two cannot drift, and so a round-trip test can
/// state its property directly: applying this delta to `older` must
/// yield `newer`.
pub fn diff_catalogues(older: &[ModuleDoc], newer: &[ModuleDoc], version: &str) -> ModuleDelta {
    let mut delta = ModuleDelta {
        version: version.to_string(),
        ..Default::default()
    };
    for m in newer {
        if !older.iter().any(|o| o.name == m.name) {
            delta.modules_added.push(m.clone());
        }
    }
    for o in older {
        let Some(n) = newer.iter().find(|n| n.name == o.name) else {
            delta.modules_removed.push(o.name.clone());
            continue;
        };
        let change = ModuleChange {
            module: o.name.clone(),
            params: diff_surface(&o.params, &n.params),
            functions: diff_surface(&o.functions, &n.functions),
        };
        if change.params.is_some() || change.functions.is_some() {
            delta.changes.push(change);
        }
    }
    delta
}

impl VersionedModules {
    /// Every supported release, oldest first.
    pub fn versions(&self) -> Vec<&str> {
        std::iter::once(self.base.version.as_str())
            .chain(self.deltas.iter().map(|d| d.version.as_str()))
            .collect()
    }

    /// The newest supported release.
    pub fn newest(&self) -> &str {
        self.deltas
            .last()
            .map(|d| d.version.as_str())
            .unwrap_or(self.base.version.as_str())
    }

    /// The catalogue as it stood at `version`, or `None` if that
    /// release is not one of the supported ones.
    pub fn at(&self, version: &str) -> Option<Vec<ModuleDoc>> {
        if version == self.base.version {
            return Some(self.base.modules.clone());
        }
        if !self.deltas.iter().any(|d| d.version == version) {
            return None;
        }
        let mut modules = self.base.modules.clone();
        for delta in &self.deltas {
            modules.retain(|m| !delta.modules_removed.contains(&m.name));
            for change in &delta.changes {
                let Some(m) = modules.iter_mut().find(|m| m.name == change.module) else {
                    continue;
                };
                if let Some(c) = &change.params {
                    apply_surface(&mut m.params, c);
                }
                if let Some(c) = &change.functions {
                    apply_surface(&mut m.functions, c);
                }
            }
            modules.extend(delta.modules_added.iter().cloned());
            if delta.version == version {
                break;
            }
        }
        canonicalize(&mut modules);
        Some(modules)
    }

    /// Which supported releases export `param` from `module`.
    ///
    /// This is what turns "unknown parameter" into a version
    /// mismatch: a name absent from the release in use but present in
    /// another is almost never a typo.
    pub fn versions_with_param(&self, module: &str, param: &str) -> Vec<String> {
        self.versions()
            .into_iter()
            .filter(|v| {
                self.at(v).is_some_and(|mods| {
                    mods.iter()
                        .any(|m| m.name == module && m.params.iter().any(|p| p.name == param))
                })
            })
            .map(|v| v.to_string())
            .collect()
    }
}

/// The vendored module catalogue: every module the pinned release
/// documents, with its exported functions and parameters.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
        let newest = builtin_versioned().newest();
        builtin_modules_at(newest).expect("the newest supported release must resolve")
    })
}

/// The vendored catalogue in full: the base release and every delta.
pub fn builtin_versioned() -> &'static VersionedModules {
    static V: std::sync::OnceLock<VersionedModules> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_str(include_str!("modules_builtin.json"))
            .expect("the vendored module catalogue must parse")
    })
}

/// The catalogue as it stood at one supported release, `None` when
/// that release is not one of them.
///
/// Reconstructing costs applying the deltas, so callers that need it
/// repeatedly should hold on to the result rather than ask again.
pub fn builtin_modules_at(version: &str) -> Option<BuiltinModules> {
    let modules = builtin_versioned().at(version)?;
    let out = BuiltinModules {
        version: version.to_string(),
        modules,
    };
    Some(out)
}

/// The provenance note for built-in documentation, when the user asks
/// to see it.
///
/// `kind` distinguishes the two catalogues, because they are pinned
/// differently: the module catalogue carries several releases and
/// follows the user's choice, while the core catalogue is one
/// vendored artefact at a single release. Naming the chosen release
/// over a core entry would be a lie about where those docs came from.
///
/// (original note follows)
/// The provenance note for built-in documentation, when the user asks
/// to see it.
///
/// It is off by default: the release is on the status bar the whole
/// time a config is open, and every warning that turns on the release
/// names it, so repeating it under every hover and every completion
/// item was the same fact a third time. Some users want it anyway —
/// reading a hover in isolation, or pasting one into a ticket — so it
/// is a setting rather than a decision made for them.
pub fn version_note(kind: &str, version: &str) -> String {
    format!(
        "\n\n*Built-in {kind} documentation from OpenSIPS {version} — set \
         `opensipsSrc` to your own source tree for version-exact docs.*"
    )
}

/// Append `note` to every core entry.
pub fn note_core(core: &mut CoreDocs, note: &str) {
    for it in core
        .functions
        .iter_mut()
        .chain(core.params.iter_mut())
        .chain(core.pvars.iter_mut())
    {
        it.doc.push_str(note);
    }
}

/// Append `note` to every entry of a module catalogue.
pub fn note_modules(modules: &mut [ModuleDoc], note: &str) {
    for m in modules.iter_mut() {
        for it in m.functions.iter_mut().chain(m.params.iter_mut()) {
            it.doc.push_str(note);
        }
    }
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
        routes: parse_core_routes_md(&read("Script-Routes.md")).unwrap_or_default(),
        statements: {
            let mut v = parse_core_statements_md(&read("Script-Statements.md")).unwrap_or_default();
            v.extend(parse_core_async_md(&read("Script-Async.md")).unwrap_or_default());
            v
        },
        socket_modifiers: harvest_socket_modifiers(tree_root),
        log_levels: LEVEL_SOURCES
            .iter()
            .map(|f| std::fs::read_to_string(tree_root.join(f)).unwrap_or_default())
            .map(|src| parse_log_levels_c(&src))
            .find(|v| !v.is_empty())
            .unwrap_or_default(),
    }
}

/// Where the level switch lives. A test holds this against the real
/// tree, so a file upstream moves fails rather than silently harvests
/// nothing.
const LEVEL_SOURCES: &[&str] = &["route.c"];

/// Cache format version: bump when `CacheFile` or the harvest
/// semantics change, so caches written by older builds self-miss.
const CACHE_SCHEMA_VERSION: u32 = 5;

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
        "Script-Routes.md",
        "Script-Statements.md",
        "Script-Async.md",
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
