//! Dependency gate for GHSA-g3hv-75wg-m249 (CVE-2026-118764), the
//! minimatch pattern-amplification advisory.
//!
//! OSV lists one affected range for the npm package minimatch:
//! introduced 0, fixed 11.0.1. The client resolves minimatch 10.2.6
//! through vscode-languageclient 11.0.0, and no vscode-languageclient
//! release accepts minimatch 11 (upstream issue
//! microsoft/vscode-languageserver-node#1938). An npm override to 11
//! is not possible either: minimatch 11 removed `Minimatch#makeRe()`,
//! which vscode-languageclient calls when it registers a file-operation
//! glob, so the override silently disables `workspace/willRenameFiles`.
//!
//! The project is not affected, and `client/osv-scanner.toml` records
//! that as an ignore entry, which Scorecard's Vulnerabilities check and
//! osv-scanner both read. The reason holds only while:
//!
//! * vscode-languageclient is the only package that depends on
//!   minimatch, because its call sites are the ones that were read;
//! * every pattern it hands to minimatch is a constant: the client's
//!   document selector carries no `pattern`, the server's file-operation
//!   globs are string literals, and the server registers no capability
//!   dynamically except file watching, which VS Code matches itself.
//!
//! This file checks those conditions, that the ignore entry has a
//! reason and has not expired, that no lockfile resolves an affected
//! minimatch without such an entry, and that the entry is removed once
//! minimatch is fixed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use syn::visit::Visit;

/// The advisory this file is about.
const ADVISORY: &str = "GHSA-g3hv-75wg-m249";

/// The first minimatch release the advisory does not cover.
const FIXED: (u64, u64, u64) = (11, 0, 1);

/// The one package whose use of minimatch the ignore entry's reason
/// covers.
const READ_CONSUMER: &str = "node_modules/vscode-languageclient";

/// The only method the server may register dynamically. Its options
/// are file-watcher globs, which vscode-languageclient passes to VS
/// Code's own watcher rather than to minimatch.
const ALLOWED_DYNAMIC_METHOD: &str = "workspace/didChangeWatchedFiles";

// ---------------------------------------------------------------------
// Versions and dates
// ---------------------------------------------------------------------

/// Whether `version` (a semver string from a lockfile) is below
/// [`FIXED`], and so covered by the advisory.
///
/// A prerelease sorts below its release, so `11.0.1-rc.1` is covered.
/// Build metadata (`+...`) does not change the order. A version that
/// does not parse is reported as covered: an entry this gate cannot
/// read must not pass it.
fn is_covered(version: &str) -> bool {
    let core_and_pre = version.split('+').next().unwrap_or(version);
    let (core, pre) = match core_and_pre.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (core_and_pre, None),
    };
    let parts: Vec<Option<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    let (major, minor, patch) = match parts.as_slice() {
        [Some(major), Some(minor), Some(patch)] => (*major, *minor, *patch),
        _ => return true,
    };
    let release = (major, minor, patch);
    release < FIXED || (release == FIXED && pre.is_some())
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Days since 1970-01-01 for a `YYYY-MM-DD` TOML local date, or `None`
/// when the text is not one.
fn parse_date(text: &str) -> Option<i64> {
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return None;
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Whether an ignore entry that runs until `until` still applies on
/// `today` (both in days since 1970-01-01). osv-scanner applies an
/// entry while the current time is before `ignoreUntil`.
fn is_in_force(until: i64, today: i64) -> bool {
    today < until
}

fn today() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs();
    i64::try_from(secs / 86_400).expect("days since 1970 fit in i64")
}

// ---------------------------------------------------------------------
// osv-scanner.toml
// ---------------------------------------------------------------------

/// One `[[IgnoredVulns]]` table.
#[derive(Debug, Default, PartialEq)]
struct IgnoreEntry {
    id: Option<String>,
    ignore_until: Option<String>,
    reason: Option<String>,
}

/// Reads the `[[IgnoredVulns]]` tables of an osv-scanner.toml.
///
/// Accepts a strict subset of TOML: comments, blank lines, the
/// `[[IgnoredVulns]]` header, `id` and `reason` as basic strings (one
/// line, or `"""` multi-line) without escapes, and `ignoreUntil` as a
/// local date. Anything else is an error, so this reader can never see
/// a different file from the one osv-scanner sees.
fn parse_ignores(text: &str) -> Result<Vec<IgnoreEntry>, String> {
    let mut entries: Vec<IgnoreEntry> = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((n, raw)) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[IgnoredVulns]]" {
            entries.push(IgnoreEntry::default());
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: not a key = value pair: {raw}", n + 1));
        };
        let (key, value) = (key.trim(), value.trim());
        let Some(entry) = entries.last_mut() else {
            return Err(format!("line {}: {key} outside [[IgnoredVulns]]", n + 1));
        };
        let string = |value: &str| -> Result<String, String> {
            let inner = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or_else(|| format!("line {}: {key} is not a string", n + 1))?;
            if inner.contains(['"', '\\']) {
                return Err(format!("line {}: {key} has a quote or escape", n + 1));
            }
            Ok(inner.to_string())
        };
        let parsed = if value == "\"\"\"" {
            let mut body = Vec::new();
            loop {
                let Some((_, more)) = lines.next() else {
                    return Err(format!("line {}: {key} is never closed", n + 1));
                };
                if more.trim() == "\"\"\"" {
                    break;
                }
                if more.contains(['"', '\\']) {
                    return Err(format!("{key}: a quote or escape inside \"\"\""));
                }
                body.push(more.trim());
            }
            body.join("\n")
        } else if key == "ignoreUntil" {
            if parse_date(value).is_none() {
                return Err(format!(
                    "line {}: ignoreUntil is not a YYYY-MM-DD date",
                    n + 1
                ));
            }
            value.to_string()
        } else {
            string(value)?
        };
        let slot = match key {
            "id" => &mut entry.id,
            "ignoreUntil" => &mut entry.ignore_until,
            "reason" => &mut entry.reason,
            _ => return Err(format!("line {}: unknown key {key}", n + 1)),
        };
        if slot.replace(parsed).is_some() {
            return Err(format!("line {}: {key} given twice", n + 1));
        }
    }
    Ok(entries)
}

/// The problems with the ignore entries for [`ADVISORY`] on `today`:
/// a missing or empty reason, a missing, unreadable or passed
/// `ignoreUntil`.
fn entry_problems(entries: &[IgnoreEntry], today: i64) -> Vec<String> {
    let mut problems = Vec::new();
    for entry in entries.iter().filter(|e| e.id.as_deref() == Some(ADVISORY)) {
        if entry.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
            problems.push(format!("{ADVISORY}: the ignore entry gives no reason"));
        }
        match entry.ignore_until.as_deref().map(parse_date) {
            None => problems.push(format!("{ADVISORY}: the ignore entry has no ignoreUntil")),
            Some(None) => problems.push(format!("{ADVISORY}: ignoreUntil is not a date")),
            Some(Some(until)) if !is_in_force(until, today) => problems.push(format!(
                "{ADVISORY}: ignoreUntil {} has passed; read the reason again, \
                 then fix minimatch or renew the entry",
                entry.ignore_until.as_deref().unwrap_or("")
            )),
            Some(Some(_)) => {}
        }
    }
    problems
}

fn ignores(entries: &[IgnoreEntry]) -> bool {
    entries.iter().any(|e| e.id.as_deref() == Some(ADVISORY))
}

// ---------------------------------------------------------------------
// npm lockfiles
// ---------------------------------------------------------------------

/// Every minimatch a parsed npm lockfile resolves, as
/// `(location, version)` pairs.
///
/// Reads both layouts npm writes: the flat `packages` map of lockfile
/// versions 2 and 3, keyed by install path, and the nested
/// `dependencies` tree of version 1 (which version 2 also carries). An
/// entry counts as minimatch when its install path ends in
/// `node_modules/minimatch`, or when it records `"name": "minimatch"`,
/// which is how npm records a package installed under an alias.
fn resolved_minimatch(lock: &serde_json::Value) -> Vec<(String, String)> {
    let mut found = Vec::new();
    if let Some(packages) = lock["packages"].as_object() {
        for (path, entry) in packages {
            let by_path =
                path == "node_modules/minimatch" || path.ends_with("/node_modules/minimatch");
            let by_name = entry["name"].as_str() == Some("minimatch") && !path.is_empty();
            if by_path || by_name {
                let version = entry["version"].as_str().unwrap_or("").to_string();
                found.push((path.clone(), version));
            }
        }
    }
    nested_minimatch(&lock["dependencies"], "", &mut found);
    found
}

/// The version 1 half of [`resolved_minimatch`]: walks a nested
/// `dependencies` object, where each entry may hold its own.
fn nested_minimatch(deps: &serde_json::Value, prefix: &str, found: &mut Vec<(String, String)>) {
    let Some(deps) = deps.as_object() else {
        return;
    };
    for (name, entry) in deps {
        let path = format!("{prefix}dependencies/{name}");
        let version = entry["version"].as_str().unwrap_or("");
        // an alias records "npm:minimatch@<version>" as the version
        if let Some(alias) = version.strip_prefix("npm:minimatch@") {
            found.push((path.clone(), alias.to_string()));
        } else if name == "minimatch" {
            found.push((path.clone(), version.to_string()));
        }
        nested_minimatch(&entry["dependencies"], &format!("{path}/"), found);
    }
}

/// The covered entries among [`resolved_minimatch`]'s results.
fn covered_minimatch(lock: &serde_json::Value) -> Vec<(String, String)> {
    resolved_minimatch(lock)
        .into_iter()
        .filter(|(_, version)| is_covered(version))
        .collect()
}

/// The install paths of the packages in a version 2 or 3 lockfile that
/// declare a dependency on minimatch (directly, or under an alias),
/// the root project included as `""`.
fn minimatch_consumers(lock: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let Some(packages) = lock["packages"].as_object() else {
        return found;
    };
    for (path, entry) in packages {
        let declares = [
            "dependencies",
            "devDependencies",
            "optionalDependencies",
            "peerDependencies",
        ]
        .iter()
        .filter_map(|field| entry[*field].as_object())
        .flatten()
        .any(|(name, range)| {
            name == "minimatch"
                || range
                    .as_str()
                    .is_some_and(|r| r.starts_with("npm:minimatch@"))
        });
        if declares {
            found.push(path.clone());
        }
    }
    found
}

/// What is wrong with one lockfile and the ignore entries beside it.
fn lockfile_problems(lock: &serde_json::Value, entries: &[IgnoreEntry]) -> Vec<String> {
    let covered = covered_minimatch(lock);
    let mut problems = Vec::new();
    if covered.is_empty() {
        if ignores(entries) {
            problems.push(format!(
                "no minimatch here is below 11.0.1 any more; remove the {ADVISORY} ignore entry"
            ));
        }
        return problems;
    }
    if !ignores(entries) {
        problems.push(format!(
            "{ADVISORY} covers {covered:?} and no ignore entry beside the lockfile says why \
             this project is not affected"
        ));
    }
    if lock["packages"].as_object().is_none() {
        problems.push("the lockfile has no packages map, so its consumers cannot be read".into());
    }
    let others: Vec<String> = minimatch_consumers(lock)
        .into_iter()
        .filter(|path| path != READ_CONSUMER)
        .collect();
    if !others.is_empty() {
        problems.push(format!(
            "{others:?} depend on minimatch; the {ADVISORY} reason covers only {READ_CONSUMER}"
        ));
    }
    problems
}

// ---------------------------------------------------------------------
// Where minimatch patterns come from
// ---------------------------------------------------------------------

/// Whether `expr` is a string literal, or one turned into a `String`
/// with `.to_string()`, `.to_owned()`, `.into()` or `String::from`.
fn is_string_literal(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Lit(lit) => matches!(lit.lit, syn::Lit::Str(_)),
        syn::Expr::MethodCall(call) => {
            call.args.is_empty()
                && ["to_string", "to_owned", "into"].contains(&call.method.to_string().as_str())
                && is_string_literal(&call.receiver)
        }
        syn::Expr::Call(call) => {
            let from = matches!(&*call.func, syn::Expr::Path(p)
                if p.path.segments.len() == 2
                    && p.path.segments[0].ident == "String"
                    && p.path.segments[1].ident == "from");
            from && call.args.len() == 1 && is_string_literal(&call.args[0])
        }
        syn::Expr::Paren(inner) => is_string_literal(&inner.expr),
        _ => false,
    }
}

/// Collects, from server source, every place a pattern could reach
/// minimatch through vscode-languageclient.
#[derive(Default)]
struct PatternSources {
    /// `FileOperationPattern { glob, .. }` expressions, as
    /// `(is a string literal, source text)`.
    globs: Vec<(bool, String)>,
    /// struct expressions for document filters, which carry a
    /// `pattern` matched by minimatch.
    document_filters: Vec<String>,
    /// the `method` of every `Registration { .. }`, as
    /// `(the literal, if it is one, source text)`.
    registrations: Vec<(Option<String>, String)>,
    /// macro bodies that name one of the types above but do not parse
    /// as expressions, so their contents cannot be checked.
    unreadable: Vec<String>,
}

impl<'ast> Visit<'ast> for PatternSources {
    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        let name = node
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        let field = |want: &str| {
            node.fields
                .iter()
                .find(|f| matches!(&f.member, syn::Member::Named(id) if id == want))
        };
        let text = quote_text(node);
        if name == "FileOperationPattern" {
            let literal = field("glob").is_some_and(|f| is_string_literal(&f.expr));
            self.globs.push((literal, text.clone()));
        }
        if name.contains("DocumentFilter") || name.contains("NotebookSelector") {
            self.document_filters.push(text.clone());
        }
        if name == "Registration" {
            let method = field("method").and_then(|f| literal_text(&f.expr));
            self.registrations.push((method, text));
        }
        syn::visit::visit_expr_struct(self, node);
    }

    /// syn does not look inside macro calls, and the server builds its
    /// file-operation filters inside `vec![]`. A body that parses as
    /// comma-separated expressions is visited like any other code; one
    /// that does not, and names a type this visitor looks for, is
    /// recorded as unreadable.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        use syn::parse::Parser;
        let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        match parser.parse2(node.tokens.clone()) {
            Ok(exprs) => {
                for expr in &exprs {
                    self.visit_expr(expr);
                }
            }
            Err(_) => {
                let tokens = node.tokens.to_string();
                if [
                    "FileOperationPattern",
                    "DocumentFilter",
                    "NotebookSelector",
                    "Registration",
                ]
                .iter()
                .any(|name| tokens.contains(name))
                {
                    self.unreadable.push(tokens);
                }
            }
        }
        syn::visit::visit_macro(self, node);
    }
}

/// The literal a [`is_string_literal`] expression wraps.
fn literal_text(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => Some(s.value()),
        syn::Expr::MethodCall(call) if is_string_literal(expr) => literal_text(&call.receiver),
        syn::Expr::Call(call) if is_string_literal(expr) => literal_text(&call.args[0]),
        syn::Expr::Paren(inner) => literal_text(&inner.expr),
        _ => None,
    }
}

fn quote_text(node: &syn::ExprStruct) -> String {
    let path: Vec<String> = node
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let fields: Vec<String> = node
        .fields
        .iter()
        .map(|f| match &f.member {
            syn::Member::Named(id) => id.to_string(),
            syn::Member::Unnamed(i) => i.index.to_string(),
        })
        .collect();
    format!("{} {{ {} }}", path.join("::"), fields.join(", "))
}

/// The problems a set of server sources has with the reachability
/// argument.
fn source_problems(sources: &PatternSources) -> Vec<String> {
    let mut problems = Vec::new();
    for tokens in &sources.unreadable {
        problems.push(format!(
            "a macro this gate cannot read builds a pattern source: {tokens}"
        ));
    }
    for (literal, text) in &sources.globs {
        if !*literal {
            problems.push(format!(
                "{text}: a file-operation glob that is not a string literal reaches minimatch"
            ));
        }
    }
    for text in &sources.document_filters {
        problems.push(format!(
            "{text}: a document filter's pattern reaches minimatch; read it against {ADVISORY}"
        ));
    }
    for (method, text) in &sources.registrations {
        if method.as_deref() != Some(ALLOWED_DYNAMIC_METHOD) {
            problems.push(format!(
                "{text}: method {method:?} is registered dynamically; its options may carry a \
                 pattern that reaches minimatch, which the {ADVISORY} reason does not cover"
            ));
        }
    }
    problems
}

fn scan_source(text: &str) -> PatternSources {
    let file = syn::parse_file(text).expect("server source parses");
    let mut sources = PatternSources::default();
    sources.visit_file(&file);
    sources
}

/// The text of every `documentSelector: [ ... ]` in client source.
fn document_selectors(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("documentSelector") {
        let after = &rest[at..];
        let Some(open) = after.find('[') else {
            found.push(after.to_string());
            break;
        };
        let mut depth = 0;
        let mut end = after.len();
        for (i, c) in after[open..].char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push(after[..end].to_string());
        rest = &after[end..];
    }
    found
}

// ---------------------------------------------------------------------
// The repository
// ---------------------------------------------------------------------

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn tracked_files() -> Vec<PathBuf> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    out.stdout
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| root().join(String::from_utf8_lossy(p).as_ref()))
        .collect()
}

fn file_name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

fn read_ignores(dir: &Path) -> Vec<IgnoreEntry> {
    let path = dir.join("osv-scanner.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_ignores(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => panic!("{}: {e}", path.display()),
    }
}

// ---------------------------------------------------------------------
// Tests of the rules, on synthetic input
// ---------------------------------------------------------------------

#[test]
fn versions_below_the_fixed_release_are_covered() {
    for version in ["0.0.1", "3.1.2", "9.0.9", "10.2.6", "11.0.0", "11.0.1-rc.1"] {
        assert!(is_covered(version), "{version} is below 11.0.1");
    }
}

#[test]
fn the_fixed_release_and_later_are_not_covered() {
    for version in ["11.0.1", "11.0.1+build.5", "11.0.4", "11.1.0", "12.0.0"] {
        assert!(!is_covered(version), "{version} is not below 11.0.1");
    }
}

#[test]
fn an_unreadable_version_is_treated_as_covered() {
    for version in ["", "latest", "11.0", "11.x.1"] {
        assert!(is_covered(version), "{version:?} must not pass the gate");
    }
}

#[test]
fn dates_count_days_from_1970() {
    assert_eq!(parse_date("1970-01-01"), Some(0));
    assert_eq!(parse_date("2000-03-01"), Some(11_017));
    assert_eq!(parse_date("2027-02-01"), Some(20_850));
    for bad in [
        "2027-2-01",
        "2027-13-01",
        "2027-02-01T00:00:00Z",
        "soon",
        "",
    ] {
        assert_eq!(parse_date(bad), None, "{bad:?}");
    }
}

#[test]
fn an_entry_applies_only_before_its_end_date() {
    assert!(
        is_in_force(100, 99),
        "the day before the end date is in force"
    );
    assert!(
        !is_in_force(100, 100),
        "the end date itself is not in force"
    );
    assert!(
        !is_in_force(100, 101),
        "the day after the end date is not in force"
    );
}

#[test]
fn a_covered_copy_is_found_in_every_lockfile_layout() {
    let flat = serde_json::json!({
        "lockfileVersion": 3,
        "packages": {
            "": { "name": "app" },
            "node_modules/minimatch": { "version": "11.0.4" },
            "node_modules/glob/node_modules/minimatch": { "version": "9.0.9" },
            "node_modules/mm": { "name": "minimatch", "version": "10.2.6" }
        }
    });
    assert_eq!(
        covered_minimatch(&flat),
        vec![
            (
                "node_modules/glob/node_modules/minimatch".to_string(),
                "9.0.9".to_string()
            ),
            ("node_modules/mm".to_string(), "10.2.6".to_string()),
        ]
    );

    let nested = serde_json::json!({
        "lockfileVersion": 1,
        "dependencies": {
            "minimatch": { "version": "11.0.4" },
            "glob": {
                "version": "7.2.3",
                "dependencies": { "minimatch": { "version": "3.1.2" } }
            },
            "mm": { "version": "npm:minimatch@10.2.6" }
        }
    });
    assert_eq!(
        covered_minimatch(&nested),
        vec![
            (
                "dependencies/glob/dependencies/minimatch".to_string(),
                "3.1.2".to_string()
            ),
            ("dependencies/mm".to_string(), "10.2.6".to_string()),
        ]
    );
}

fn entry(id: &str, until: Option<&str>, reason: Option<&str>) -> IgnoreEntry {
    IgnoreEntry {
        id: Some(id.to_string()),
        ignore_until: until.map(str::to_string),
        reason: reason.map(str::to_string),
    }
}

fn affected_lock(extra: serde_json::Value) -> serde_json::Value {
    let mut packages = serde_json::json!({
        "": { "name": "app", "dependencies": { "vscode-languageclient": "^11.0.0" } },
        "node_modules/vscode-languageclient": {
            "version": "11.0.0",
            "dependencies": { "minimatch": "^10.2.6" }
        },
        "node_modules/minimatch": { "version": "10.2.6" }
    });
    if let (Some(p), Some(e)) = (packages.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    serde_json::json!({ "lockfileVersion": 3, "packages": packages })
}

#[test]
fn an_affected_minimatch_needs_an_ignore_entry() {
    let lock = affected_lock(serde_json::json!({}));
    let problems = lockfile_problems(&lock, &[]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("no ignore entry"), "{problems:?}");

    let other = [entry("GHSA-0000-0000-0000", Some("2099-01-01"), Some("x"))];
    assert_eq!(lockfile_problems(&lock, &other).len(), 1);

    let ok = [entry(ADVISORY, Some("2099-01-01"), Some("not reachable"))];
    assert_eq!(lockfile_problems(&lock, &ok), Vec::<String>::new());
}

#[test]
fn another_minimatch_consumer_voids_the_exception() {
    let ok = [entry(ADVISORY, Some("2099-01-01"), Some("not reachable"))];
    let glob = affected_lock(serde_json::json!({
        "node_modules/glob": { "version": "10.0.0", "dependencies": { "minimatch": "^9.0.0" } }
    }));
    let problems = lockfile_problems(&glob, &ok);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("node_modules/glob"), "{problems:?}");

    let alias = affected_lock(serde_json::json!({
        "": { "name": "app", "devDependencies": { "mm": "npm:minimatch@^10.0.0" } }
    }));
    let problems = lockfile_problems(&alias, &ok);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("[\"\"]"), "{problems:?}");
}

#[test]
fn a_fixed_minimatch_makes_the_ignore_entry_stale() {
    let fixed = serde_json::json!({
        "lockfileVersion": 3,
        "packages": { "node_modules/minimatch": { "version": "11.0.1" } }
    });
    let ok = [entry(ADVISORY, Some("2099-01-01"), Some("not reachable"))];
    let problems = lockfile_problems(&fixed, &ok);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("remove the"), "{problems:?}");
    assert_eq!(lockfile_problems(&fixed, &[]), Vec::<String>::new());

    let none = serde_json::json!({
        "lockfileVersion": 3,
        "packages": { "node_modules/semver": { "version": "7.8.5" } }
    });
    assert_eq!(lockfile_problems(&none, &ok).len(), 1);
}

#[test]
fn an_entry_without_a_reason_or_past_its_date_is_refused() {
    let today = parse_date("2026-11-03").unwrap();
    assert_eq!(
        entry_problems(&[entry(ADVISORY, Some("2027-02-01"), Some("why"))], today),
        Vec::<String>::new()
    );
    for (e, want) in [
        (entry(ADVISORY, Some("2027-02-01"), None), "no reason"),
        (entry(ADVISORY, Some("2027-02-01"), Some("  ")), "no reason"),
        (entry(ADVISORY, None, Some("why")), "no ignoreUntil"),
        (
            entry(ADVISORY, Some("2026-11-03"), Some("why")),
            "has passed",
        ),
        (
            entry(ADVISORY, Some("2026-01-01"), Some("why")),
            "has passed",
        ),
    ] {
        let problems = entry_problems(&[e], today);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains(want), "{problems:?} lacks {want}");
    }
}

#[test]
fn the_ignore_file_reader_accepts_only_what_it_understands() {
    let text = "# note\n[[IgnoredVulns]]\nid = \"GHSA-g3hv-75wg-m249\"\n\
                ignoreUntil = 2027-02-01\nreason = \"\"\"\n  one\n  two\n\"\"\"\n";
    assert_eq!(
        parse_ignores(text),
        Ok(vec![entry(ADVISORY, Some("2027-02-01"), Some("one\ntwo"))])
    );
    for bad in [
        "id = \"x\"\n",
        "[[IgnoredVulns]]\nid = x\n",
        "[[IgnoredVulns]]\nid = \"a\\\"b\"\n",
        "[[IgnoredVulns]]\nignoreUntil = \"2027-02-01\"\n",
        "[[IgnoredVulns]]\nreason = \"\"\"\nopen\n",
        "[[IgnoredVulns]]\nid = \"a\"\nid = \"b\"\n",
        "[[IgnoredVulns]]\neffectiveUntil = 2027-02-01\n",
        "[[PackageOverrides]]\n",
    ] {
        assert!(parse_ignores(bad).is_err(), "{bad:?} was accepted");
    }
}

#[test]
fn only_literal_file_operation_globs_and_watcher_registrations_pass() {
    let good = r#"
        fn f() {
            let _ = FileOperationPattern { glob: "**/*".to_string(), matches: None, options: None };
            let _ = Registration { id: "w".into(), method: "workspace/didChangeWatchedFiles".into(), register_options: None };
        }
    "#;
    let sources = scan_source(good);
    let in_vec = scan_source(
        r#"fn f() { let _ = vec![FileOperationFilter { scheme: None, pattern: FileOperationPattern { glob: g(), matches: None, options: None } }]; }"#,
    );
    assert_eq!(in_vec.globs.len(), 1, "a glob inside vec![] was not seen");
    assert_eq!(source_problems(&in_vec).len(), 1);
    let opaque = scan_source(r#"fn f() { m!(FileOperationPattern => x); }"#);
    assert_eq!(
        source_problems(&opaque).len(),
        1,
        "an unreadable macro passed"
    );
    assert_eq!(sources.globs.len(), 1);
    assert_eq!(source_problems(&sources), Vec::<String>::new());

    for (bad, want) in [
        (
            r#"fn f(g: String) { let _ = FileOperationPattern { glob: g, matches: None, options: None }; }"#,
            "not a string literal",
        ),
        (
            r#"fn f() { let _ = FileOperationPattern { glob: format!("**/*.{}", "cfg"), matches: None, options: None }; }"#,
            "not a string literal",
        ),
        (
            r#"fn f() { let _ = TextDocumentFilter { language: None, scheme: None, pattern: Some("*".into()) }; }"#,
            "document filter",
        ),
        (
            r#"fn f() { let _ = Registration { id: "r".into(), method: "workspace/willRenameFiles".into(), register_options: None }; }"#,
            "registered dynamically",
        ),
        (
            r#"fn f(m: String) { let _ = Registration { id: "r".into(), method: m, register_options: None }; }"#,
            "registered dynamically",
        ),
    ] {
        let problems = source_problems(&scan_source(bad));
        assert_eq!(problems.len(), 1, "{bad}: {problems:?}");
        assert!(problems[0].contains(want), "{problems:?} lacks {want}");
    }
}

#[test]
fn document_selectors_are_read_whole() {
    let text = "a\n  documentSelector: [{ language: 'x' },\n  { pattern: '**/*' }],\n b";
    assert_eq!(
        document_selectors(text),
        vec!["documentSelector: [{ language: 'x' },\n  { pattern: '**/*' }]".to_string()]
    );
}

// ---------------------------------------------------------------------
// The gate, on the repository as committed
// ---------------------------------------------------------------------

/// Every tracked npm lockfile either resolves no minimatch the advisory
/// covers, or sits beside an osv-scanner.toml ignore entry whose
/// conditions hold; and no ignore entry outlives the vulnerable copy.
#[test]
fn every_lockfile_is_fixed_or_carries_a_justified_exception() {
    let files = tracked_files();

    // Lockfile formats this gate cannot parse must not exist, or an
    // affected copy could sit in one unseen.
    let unread: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            matches!(
                file_name(p),
                "yarn.lock" | "pnpm-lock.yaml" | "bun.lock" | "bun.lockb"
            )
        })
        .collect();
    assert!(
        unread.is_empty(),
        "lockfiles this gate does not read: {unread:?}"
    );

    let lockfiles: Vec<&PathBuf> = files
        .iter()
        .filter(|p| matches!(file_name(p), "package-lock.json" | "npm-shrinkwrap.json"))
        .collect();
    // POSITIVE CONTROL: the client's lockfile is tracked and read, so a
    // clean result means "checked", not "nothing was read".
    let client = root().join("client/package-lock.json");
    assert!(
        lockfiles.contains(&&client),
        "client/package-lock.json is not tracked: {lockfiles:?}"
    );

    let mut problems = Vec::new();
    for lockfile in &lockfiles {
        let text = std::fs::read_to_string(lockfile).unwrap();
        let lock: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not JSON: {e}", lockfile.display()));
        assert!(
            lock["packages"].as_object().is_some_and(|p| p.len() > 1),
            "{} lists no packages",
            lockfile.display()
        );
        let entries = read_ignores(lockfile.parent().unwrap());
        for p in lockfile_problems(&lock, &entries)
            .into_iter()
            .chain(entry_problems(&entries, today()))
        {
            problems.push(format!("{}: {p}", lockfile.display()));
        }
    }

    // an ignore file with no lockfile beside it ignores nothing osv-scanner
    // reads here, so it can only be stale
    for config in files.iter().filter(|p| file_name(p) == "osv-scanner.toml") {
        let dir = config.parent().unwrap();
        if !lockfiles.iter().any(|l| l.parent() == Some(dir)) {
            problems.push(format!("{}: no lockfile beside it", config.display()));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The patterns vscode-languageclient hands to minimatch are all
/// constants, which is what the ignore entry's reason claims.
#[test]
fn every_pattern_that_reaches_minimatch_is_a_constant() {
    let files = tracked_files();
    let src: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.starts_with(root().join("src")) && file_name(p).ends_with(".rs"))
        .collect();
    let mut sources = PatternSources::default();
    for path in &src {
        let one = scan_source(&std::fs::read_to_string(path).unwrap());
        sources.globs.extend(one.globs);
        sources.document_filters.extend(one.document_filters);
        sources.registrations.extend(one.registrations);
        sources.unreadable.extend(one.unreadable);
    }
    // POSITIVE CONTROL: the server does register a file-operation glob
    // and a watcher, so the visitor is finding them.
    assert!(
        !sources.globs.is_empty(),
        "no FileOperationPattern found in {src:?}"
    );
    assert!(!sources.registrations.is_empty(), "no Registration found");
    let mut problems = source_problems(&sources);

    let ts: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.starts_with(root().join("client/src")) && file_name(p).ends_with(".ts"))
        .collect();
    let mut selectors = Vec::new();
    for path in &ts {
        selectors.extend(document_selectors(&std::fs::read_to_string(path).unwrap()));
    }
    assert!(!selectors.is_empty(), "no documentSelector found in {ts:?}");
    for selector in selectors {
        if selector.contains("pattern") || !selector.ends_with(']') {
            problems.push(format!(
                "client document selector {selector:?} may carry a pattern that reaches minimatch"
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
