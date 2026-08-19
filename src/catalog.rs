//! Module documentation catalog, harvested from the OpenSIPS docbook
//! sources (`modules/<name>/doc/<name>_admin.xml`).

use std::path::Path;

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
        if let Ok(md) = std::fs::read_to_string(&readme)
            && let Ok(m) = parse_readme_md(&name, &md)
            && (!m.params.is_empty() || !m.functions.is_empty())
        {
            out.push(m);
            continue;
        }
        let admin = e.path().join("doc").join(format!("{name}_admin.xml"));
        if let Ok(xml) = std::fs::read_to_string(&admin)
            && let Ok(m) = parse_admin_xml(&name, &xml)
        {
            out.push(m);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
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

    #[derive(PartialEq)]
    enum Section {
        Params,
        Functions,
        Other,
    }
    let mut section = Section::Other;
    let mut out = ModuleDoc {
        name: module.to_string(),
        ..Default::default()
    };
    // (is_param, name, detail, doc-lines, doc-finished)
    let mut cur: Option<(bool, String, String, Vec<String>, bool)> = None;

    let flush = |cur: &mut Option<(bool, String, String, Vec<String>, bool)>,
                 out: &mut ModuleDoc| {
        if let Some((is_param, name, detail, lines, _)) = cur.take() {
            let doc = sanitize_doc(
                &lines
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            let item = Item { name, detail, doc };
            if is_param {
                out.params.push(item);
            } else {
                out.functions.push(item);
            }
        }
    };

    for line in md.lines() {
        if let Some(h3) = line.strip_prefix("### ") {
            flush(&mut cur, &mut out);
            section = match h3.trim() {
                "Exported Parameters" => Section::Params,
                "Exported Functions" => Section::Functions,
                _ => Section::Other,
            };
            continue;
        }
        if let Some(h4) = line.strip_prefix("#### ") {
            flush(&mut cur, &mut out);
            let heading = h4.trim();
            match section {
                Section::Params => {
                    let (name, detail) = match heading.split_once(" (") {
                        Some((n, rest)) => (
                            n.trim().to_string(),
                            rest.trim_end_matches(')').trim().to_string(),
                        ),
                        None => (heading.to_string(), String::new()),
                    };
                    cur = Some((true, name, detail, Vec::new(), false));
                }
                Section::Functions => {
                    let name = heading
                        .split('(')
                        .next()
                        .unwrap_or(heading)
                        .trim()
                        .to_string();
                    cur = Some((false, name, heading.to_string(), Vec::new(), false));
                }
                Section::Other => {}
            }
            continue;
        }
        if line.starts_with('#') {
            // any other heading level ends the current item
            flush(&mut cur, &mut out);
            section = Section::Other;
            continue;
        }
        if let Some((_, _, _, lines, finished)) = cur.as_mut() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !lines.is_empty() {
                    *finished = true; // first paragraph complete
                }
            } else if !*finished {
                lines.push(trimmed.to_string());
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

/// A cheap change-detector for a source tree: canonical path plus the
/// modification times of `modules/` and `docs/manual/` (module or doc
/// additions/removals bump the directory mtimes).
pub fn tree_fingerprint(tree_root: &Path) -> String {
    use std::time::UNIX_EPOCH;
    let mtime = |p: &Path| -> u128 {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0)
    };
    let canon = std::fs::canonicalize(tree_root).unwrap_or_else(|_| tree_root.to_path_buf());
    let raw = format!(
        "{}|{}|{}",
        canon.display(),
        mtime(&canon.join("modules")),
        mtime(&canon.join("docs").join("manual")),
    );
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
    let f = cache_dir.join(format!("{}.json", tree_fingerprint(tree_root)));
    let c = CacheFile {
        modules: modules.to_vec(),
        core: core.clone(),
    };
    let bytes = serde_json::to_vec(&c).map_err(|e| e.to_string())?;
    // atomic publish: concurrent servers may write the same file, and
    // readers must never see a torn cache — write-then-rename
    let tmp = cache_dir.join(format!(
        ".{}.{}.tmp",
        tree_fingerprint(tree_root),
        std::process::id()
    ));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &f).map_err(|e| e.to_string())
}
