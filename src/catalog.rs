//! Module documentation catalog, harvested from the OpenSIPS docbook
//! sources (`modules/<name>/doc/<name>_admin.xml`).

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Bare name (`kv_bucket`, `nats_kv_get`).
    pub name: String,
    /// Human detail: the param type (`string`) or function signature.
    pub detail: String,
    /// First documentation paragraph, whitespace-collapsed.
    pub doc: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModuleDoc {
    pub name: String,
    pub params: Vec<Item>,
    pub functions: Vec<Item>,
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
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
        // docbook (feature-branch modules) first, 4.x markdown otherwise
        let admin = e.path().join("doc").join(format!("{name}_admin.xml"));
        if let Ok(xml) = std::fs::read_to_string(&admin) {
            if let Ok(m) = parse_admin_xml(&name, &xml) {
                out.push(m);
                continue;
            }
        }
        let readme = e.path().join("README.md");
        if let Ok(md) = std::fs::read_to_string(&readme) {
            if let Ok(m) = parse_readme_md(&name, &md) {
                out.push(m);
            }
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

    let mut flush = |cur: &mut Option<(bool, String, String, Vec<String>, bool)>,
                     out: &mut ModuleDoc| {
        if let Some((is_param, name, detail, lines, _)) = cur.take() {
            let doc = lines
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
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
