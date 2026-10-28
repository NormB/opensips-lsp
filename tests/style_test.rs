//! Style gate for the rules that rustfmt and clippy cannot see.
//!
//! The Google Rust Style Guide, which binds this repository together with
//! the Rust Style Guide and the Rust API Guidelines, asks for things that
//! no lint checks:
//!
//! 1. No glob imports, except `use super::*` in an inline module,
//!    `use Enum::*` inside a function, and `pub use module::*` in a
//!    non-leaf module (`lib.rs` or `mod.rs`).
//! 2. Every `unsafe` block, function, impl, trait and extern block carries
//!    a `// SAFETY:` comment (or, for a function, a `# Safety` doc section)
//!    in the comment lines directly above it.
//! 3. Every `assert!` and `debug_assert!` carries a context message.
//! 4. A function is called through its module (`analyze::calls(..)`), so
//!    no `use` imports a function this repository defines.
//! 5. Outside the tests a panic says why, so every crate root that is not
//!    a test denies `clippy::unwrap_used`: the library, the binary, the
//!    examples and the fuzz targets.
//! 6. No `panic::catch_unwind`: the guide prohibits it in first-party code.
//!
//! The checks read the syntax tree, so a rule named in a comment or a
//! string is not mistaken for code. Each rule is a pure function of a
//! source text, driven both by the fixtures below and by every Rust file
//! in the repository.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, Macro, Token, UseTree, Visibility};

/// Which of the rules a finding breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rule {
    /// A glob import outside the three allowed forms.
    GlobImport,
    /// An `unsafe` item or block with no `SAFETY:` comment above it.
    UnsafeWithoutSafety,
    /// An `assert!` or `debug_assert!` with no message.
    AssertWithoutMessage,
    /// A `use` that imports a function this repository defines.
    FunctionImport,
    /// A crate root outside the tests that does not deny `unwrap()`.
    UnwrapNotDenied,
    /// A path that names `catch_unwind`.
    CatchUnwind,
}

/// Whether a file is a crate root that must deny `clippy::unwrap_used`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnwrapRule {
    /// The library, the binary, an example or a fuzz target.
    MustDeny,
    /// A test, or a module that is not a crate root.
    NotApplicable,
}

/// The top-level functions this repository defines, as
/// `(module, function)`: those of `src/<module>.rs`, and of the tests'
/// shared `common` module.
type FnIndex = BTreeSet<(String, String)>;

/// Whether a file is a non-leaf module, which may hold `pub use m::*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModuleKind {
    /// A module that implements things.
    Leaf,
    /// `lib.rs` or `mod.rs`: declarations and re-exports.
    NonLeaf,
}

/// One rule broken at one line.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Finding {
    line: usize,
    rule: Rule,
}

/// Every finding in `source`, in source order.
///
/// # Errors
///
/// Returns the parse error when `source` is not a Rust file.
fn check_source(
    source: &str,
    kind: ModuleKind,
    unwrap: UnwrapRule,
    fns: &FnIndex,
) -> Result<Vec<Finding>, syn::Error> {
    let file = syn::parse_file(source)?;
    let mut checker = Checker {
        lines: source.lines().collect(),
        kind,
        fns,
        inline_modules: 0,
        functions: 0,
        found: Vec::new(),
    };
    if unwrap == UnwrapRule::MustDeny && !denies_unwrap(&file) {
        checker.flag(1, Rule::UnwrapNotDenied);
    }
    checker.visit_file(&file);
    checker.found.sort_by_key(|f| (f.line, f.rule));
    checker.found.dedup();
    Ok(checker.found)
}

/// Walks one file's syntax tree and records each rule it breaks.
struct Checker<'a> {
    lines: Vec<&'a str>,
    kind: ModuleKind,
    fns: &'a FnIndex,
    /// How many inline (`mod m { .. }`) modules enclose the current item.
    inline_modules: usize,
    /// How many function bodies enclose the current item.
    functions: usize,
    found: Vec<Finding>,
}

impl Checker<'_> {
    fn flag(&mut self, line: usize, rule: Rule) {
        self.found.push(Finding { line, rule });
    }

    /// Whether the comment lines directly above `line` (1-based), or the
    /// line itself, say why the `unsafe` there is sound.
    fn has_safety_comment(&self, line: usize) -> bool {
        let says_why = |text: &str| text.contains("SAFETY:") || text.contains("# Safety");
        if self.lines.get(line - 1).is_some_and(|text| says_why(text)) {
            return true;
        }
        let mut at = line - 1;
        while at > 0 {
            let text = self.lines[at - 1].trim_start();
            if !(text.starts_with("//") || text.starts_with("#[")) {
                return false;
            }
            if says_why(text) {
                return true;
            }
            at -= 1;
        }
        false
    }

    fn check_unsafe(&mut self, line: usize) {
        if !self.has_safety_comment(line) {
            self.flag(line, Rule::UnsafeWithoutSafety);
        }
    }

    /// Records each glob in `tree` that none of the allowed forms covers.
    fn check_use(&mut self, tree: &UseTree, prefix: &mut Vec<String>, public: bool) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.check_use(&path.tree, prefix, public);
                prefix.pop();
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.check_use(item, prefix, public);
                }
            }
            UseTree::Glob(glob) => {
                if !self.glob_allowed(prefix, public) {
                    self.flag(glob.span().start().line, Rule::GlobImport);
                }
            }
            UseTree::Name(name) => {
                if name.ident == "catch_unwind" {
                    self.flag(name.ident.span().start().line, Rule::CatchUnwind);
                }
                if self.names_function(prefix, &name.ident) {
                    self.flag(name.ident.span().start().line, Rule::FunctionImport);
                }
            }
            UseTree::Rename(rename) => {
                if rename.ident == "catch_unwind" {
                    self.flag(rename.ident.span().start().line, Rule::CatchUnwind);
                }
                if self.names_function(prefix, &rename.ident) {
                    self.flag(rename.ident.span().start().line, Rule::FunctionImport);
                }
            }
        }
    }

    /// Whether `prefix::ident` is a function this repository defines.
    fn names_function(&self, prefix: &[String], ident: &syn::Ident) -> bool {
        let module = match prefix {
            [krate, module] if krate == "opensips_lsp" || krate == "crate" => module,
            [module] if module == "common" => module,
            _ => return false,
        };
        self.fns.contains(&(module.clone(), ident.to_string()))
    }

    fn glob_allowed(&self, prefix: &[String], public: bool) -> bool {
        let super_in_inline_module = self.inline_modules > 0 && prefix == ["super"];
        let enum_in_function = self.functions > 0
            && prefix
                .last()
                .and_then(|last| last.chars().next())
                .is_some_and(char::is_uppercase);
        let reexport_from_non_leaf = public
            && self.kind == ModuleKind::NonLeaf
            && self.inline_modules == 0
            && self.functions == 0;
        super_in_inline_module || enum_in_function || reexport_from_non_leaf
    }

    /// Records an `assert!` or `debug_assert!` with no message, then looks
    /// inside the macro's arguments for more.
    fn check_macro(&mut self, mac: &Macro) {
        let name = mac.path.segments.last().map(|s| s.ident.to_string());
        if matches!(name.as_deref(), Some("assert" | "debug_assert"))
            && !has_message(mac.tokens.clone())
        {
            self.flag(mac.path.span().start().line, Rule::AssertWithoutMessage);
        }
        // A macro's arguments are tokens to syn; read them as expressions
        // or as statements so that an assert inside another macro is seen.
        let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
        if let Ok(args) = mac.parse_body_with(parser) {
            for arg in &args {
                self.visit_expr(arg);
            }
        } else if let Ok(stmts) = mac.parse_body_with(syn::Block::parse_within) {
            for stmt in &stmts {
                self.visit_stmt(stmt);
            }
        }
    }
}

/// Whether a crate root holds `#![deny(clippy::unwrap_used)]` (or
/// `forbid`).
fn denies_unwrap(file: &syn::File) -> bool {
    file.attrs.iter().any(|attr| {
        let level = attr.path().is_ident("deny") || attr.path().is_ident("forbid");
        let names_lint = attr.meta.require_list().is_ok_and(|list| {
            list.tokens
                .to_string()
                .replace(' ', "")
                .contains("clippy::unwrap_used")
        });
        matches!(attr.style, syn::AttrStyle::Inner(_)) && level && names_lint
    })
}

/// The top-level functions of `src/*.rs` and `tests/common/mod.rs`.
fn function_index(root: &Path) -> FnIndex {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let src = root.join("src");
    let entries =
        fs::read_dir(&src).unwrap_or_else(|e| panic!("{} is not readable: {e}", src.display()));
    for entry in entries {
        let path = entry.expect("a directory entry is readable").path();
        let stem = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned);
        if let Some(stem) = stem.filter(|_| path.extension().is_some_and(|e| e == "rs")) {
            files.push((stem, path));
        }
    }
    files.push(("common".to_owned(), root.join("tests/common/mod.rs")));
    let mut index = FnIndex::new();
    for (module, path) in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} is not readable: {e}", path.display()));
        let file = syn::parse_file(&source)
            .unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()));
        for item in file.items {
            if let syn::Item::Fn(f) = item {
                index.insert((module.clone(), f.sig.ident.to_string()));
            }
        }
    }
    index
}

/// Whether an assertion's tokens hold anything after the condition.
fn has_message(tokens: TokenStream) -> bool {
    let mut after_comma = false;
    for token in tokens {
        match token {
            TokenTree::Punct(p) if p.as_char() == ',' && !after_comma => after_comma = true,
            _ if after_comma => return true,
            _ => {}
        }
    }
    false
}

impl<'ast> Visit<'ast> for Checker<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if let Some(unsafety) = node.unsafety {
            self.check_unsafe(unsafety.span.start().line);
        }
        let inline = node.content.is_some();
        self.inline_modules += usize::from(inline);
        syn::visit::visit_item_mod(self, node);
        self.inline_modules -= usize::from(inline);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if let Some(unsafety) = node.sig.unsafety {
            self.check_unsafe(unsafety.span.start().line);
        }
        self.functions += 1;
        syn::visit::visit_item_fn(self, node);
        self.functions -= 1;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if let Some(unsafety) = node.sig.unsafety {
            self.check_unsafe(unsafety.span.start().line);
        }
        self.functions += 1;
        syn::visit::visit_impl_item_fn(self, node);
        self.functions -= 1;
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if let Some(unsafety) = node.sig.unsafety {
            self.check_unsafe(unsafety.span.start().line);
        }
        self.functions += 1;
        syn::visit::visit_trait_item_fn(self, node);
        self.functions -= 1;
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if let Some(unsafety) = node.unsafety {
            self.check_unsafe(unsafety.span.start().line);
        }
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if let Some(unsafety) = node.unsafety {
            self.check_unsafe(unsafety.span.start().line);
        }
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_foreign_mod(&mut self, node: &'ast syn::ItemForeignMod) {
        if let Some(unsafety) = node.unsafety {
            self.check_unsafe(unsafety.span.start().line);
        }
        syn::visit::visit_item_foreign_mod(self, node);
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.check_unsafe(node.unsafe_token.span.start().line);
        syn::visit::visit_expr_unsafe(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let public = !matches!(node.vis, Visibility::Inherited);
        self.check_use(&node.tree, &mut Vec::new(), public);
        syn::visit::visit_item_use(self, node);
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        if let Some(last) = node.segments.last().filter(|s| s.ident == "catch_unwind") {
            self.flag(last.ident.span().start().line, Rule::CatchUnwind);
        }
        syn::visit::visit_path(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        self.check_macro(node);
        syn::visit::visit_macro(self, node);
    }
}

/// The functions the fixtures may import: `analyze::calls` and
/// `common::wait_for`.
fn fixture_fns() -> FnIndex {
    [
        ("analyze", "calls"),
        ("analyze", "classify"),
        ("common", "wait_for"),
    ]
    .iter()
    .map(|(m, f)| ((*m).to_owned(), (*f).to_owned()))
    .collect()
}

/// The rules `source` breaks, as `(line, rule)` pairs.
fn findings(source: &str, kind: ModuleKind) -> Vec<(usize, Rule)> {
    findings_as(source, kind, UnwrapRule::NotApplicable)
}

/// The rules `source` breaks as a file with the given unwrap rule.
fn findings_as(source: &str, kind: ModuleKind, unwrap: UnwrapRule) -> Vec<(usize, Rule)> {
    check_source(source, kind, unwrap, &fixture_fns())
        .expect("each fixture is a Rust file")
        .into_iter()
        .map(|f| (f.line, f.rule))
        .collect()
}

#[test]
fn a_glob_import_is_found() {
    let source = "use std::collections::*;\nuse std::io::{self, prelude::*};\n";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [(1, Rule::GlobImport), (2, Rule::GlobImport)]
    );
}

#[test]
fn the_allowed_glob_imports_are_not_found() {
    let source = "\
pub use inner::*;
mod tests {
    use super::*;
}
fn f(c: Color) {
    use Color::*;
}
";
    assert_eq!(findings(source, ModuleKind::NonLeaf), []);
}

#[test]
fn a_glob_is_allowed_only_in_its_own_place() {
    // `pub use m::*` in a leaf module, `use super::*` at file level and
    // `use module::*` (lower case) inside a function are all banned.
    let source = "\
pub use inner::*;
use super::*;
fn f() {
    use std::mem::*;
}
";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [
            (1, Rule::GlobImport),
            (2, Rule::GlobImport),
            (4, Rule::GlobImport)
        ]
    );
}

#[test]
fn unsafe_without_a_safety_comment_is_found() {
    let source = "\
unsafe fn raw() {}
unsafe impl Send for S {}
unsafe trait T {}
unsafe extern \"C\" {}
fn f() {
    let x = 1;
    unsafe { raw() };
}
";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [
            (1, Rule::UnsafeWithoutSafety),
            (2, Rule::UnsafeWithoutSafety),
            (3, Rule::UnsafeWithoutSafety),
            (4, Rule::UnsafeWithoutSafety),
            (7, Rule::UnsafeWithoutSafety)
        ]
    );
}

#[test]
fn unsafe_with_a_safety_comment_is_not_found() {
    let source = "\
/// Reads nothing.
///
/// # Safety
///
/// Any caller is sound.
unsafe fn raw() {}
// SAFETY: S holds no thread-bound state.
#[allow(unused)]
unsafe impl Send for S {}
fn f() {
    // SAFETY: raw has no preconditions.
    unsafe { raw() };
    let y = unsafe { raw() }; // SAFETY: as above.
}
";
    assert_eq!(findings(source, ModuleKind::Leaf), []);
}

#[test]
fn a_safety_comment_must_sit_directly_above() {
    // A blank or code line between the comment and the block breaks the
    // link between them.
    let source = "\
fn f() {
    // SAFETY: about something else.
    let x = 1;
    unsafe { raw() };
}
";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [(4, Rule::UnsafeWithoutSafety)]
    );
}

#[test]
fn an_assert_without_a_message_is_found() {
    let source = "\
fn f() {
    assert!(true);
    debug_assert!(1 < 2,);
    std::assert!(matches!(Some(1), Some(_)));
}
";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [
            (2, Rule::AssertWithoutMessage),
            (3, Rule::AssertWithoutMessage),
            (4, Rule::AssertWithoutMessage)
        ]
    );
}

#[test]
fn an_assert_with_a_message_is_not_found() {
    let source = "\
fn f(v: &[u8]) {
    assert!(v.is_empty(), \"{} bytes\", v.len());
    debug_assert!(f(&[1, 2]), \"a call with commas inside\");
    assert_eq!(1, 1);
}
";
    assert_eq!(findings(source, ModuleKind::Leaf), []);
}

#[test]
fn an_assert_inside_another_macro_is_found() {
    let source = "\
fn f() {
    let v = vec![{ assert!(true); 1 }];
    tokio::spawn(async { assert!(false) });
    my_macro! {
        let x = 1;
        assert!(x == 1);
    }
}
";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [
            (2, Rule::AssertWithoutMessage),
            (3, Rule::AssertWithoutMessage),
            (6, Rule::AssertWithoutMessage)
        ]
    );
}

#[test]
fn a_rule_named_in_a_comment_or_string_is_not_code() {
    let source = "\
// use std::io::*; and unsafe { } and assert!(x)
const S: &str = \"assert!(x); use a::*; unsafe {}\";
";
    assert_eq!(findings(source, ModuleKind::Leaf), []);
}

#[test]
fn an_imported_function_is_found() {
    let source = "\
use opensips_lsp::analyze::calls;
use crate::analyze::{self, Class, classify};
use common::wait_for as wait;
";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [
            (1, Rule::FunctionImport),
            (2, Rule::FunctionImport),
            (3, Rule::FunctionImport)
        ]
    );
}

#[test]
fn a_module_or_a_type_import_is_not_a_function_import() {
    // `calls` is a function of `analyze`, but not of `logic`, and a
    // path from another crate is not one of this repository's functions.
    let source = "\
use opensips_lsp::analyze;
use opensips_lsp::analyze::Class;
use opensips_lsp::logic::calls;
use common::Server;
use other::analyze::calls;
fn f() {
    analyze::calls(\"\");
}
";
    assert_eq!(findings(source, ModuleKind::Leaf), []);
}

#[test]
fn a_crate_root_outside_the_tests_must_deny_unwrap() {
    let bare = "//! A crate.\nfn main() {}\n";
    let warned = "#![warn(clippy::unwrap_used)]\nfn main() {}\n";
    let denied = "#![deny(missing_docs, clippy::unwrap_used)]\nfn main() {}\n";
    let forbidden = "#![forbid(clippy::unwrap_used)]\nfn main() {}\n";
    let must = UnwrapRule::MustDeny;
    assert_eq!(
        findings_as(bare, ModuleKind::Leaf, must),
        [(1, Rule::UnwrapNotDenied)]
    );
    assert_eq!(
        findings_as(warned, ModuleKind::Leaf, must),
        [(1, Rule::UnwrapNotDenied)]
    );
    assert_eq!(findings_as(denied, ModuleKind::Leaf, must), []);
    assert_eq!(findings_as(forbidden, ModuleKind::Leaf, must), []);
    assert_eq!(
        findings_as(bare, ModuleKind::Leaf, UnwrapRule::NotApplicable),
        []
    );
}

#[test]
fn catch_unwind_is_found_however_it_is_named() {
    let source = "\
use std::panic::catch_unwind;
fn f() {
    let _ = std::panic::catch_unwind(|| 1);
    let _ = panic::catch_unwind(|| 2);
}
";
    assert_eq!(
        findings(source, ModuleKind::Leaf),
        [
            (1, Rule::CatchUnwind),
            (3, Rule::CatchUnwind),
            (4, Rule::CatchUnwind)
        ]
    );
}

/// Every Rust file the repository tracks under the directories that hold
/// Rust code.
fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = ["src", "tests", "examples", "fuzz/fuzz_targets"]
        .iter()
        .map(|d| root.join(d))
        .collect();
    while let Some(dir) = dirs.pop() {
        let entries =
            fs::read_dir(&dir).unwrap_or_else(|e| panic!("{} is not readable: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a directory entry is readable").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Findings the tree still holds, by file and rule, while the judgment
/// changes that remove them land. The count must match exactly: a new
/// finding fails, and so does a fix that does not lower its entry here.
const PENDING: &[(&str, Rule, usize)] = &[
    ("fuzz/fuzz_targets/cfg.rs", Rule::AssertWithoutMessage, 5),
    ("tests/analyze_test.rs", Rule::AssertWithoutMessage, 12),
    (
        "tests/assistance_toggle_e2e_test.rs",
        Rule::AssertWithoutMessage,
        2,
    ),
    ("tests/c_params_test.rs", Rule::AssertWithoutMessage, 1),
    ("tests/cache_test.rs", Rule::AssertWithoutMessage, 4),
    ("tests/call_graph_test.rs", Rule::AssertWithoutMessage, 1),
    ("tests/catalog_test.rs", Rule::AssertWithoutMessage, 13),
    (
        "tests/checker_flag_doc_test.rs",
        Rule::AssertWithoutMessage,
        3,
    ),
    ("tests/config_test.rs", Rule::AssertWithoutMessage, 7),
    ("tests/core_docs_test.rs", Rule::AssertWithoutMessage, 8),
    ("tests/diag_currency_test.rs", Rule::AssertWithoutMessage, 1),
    ("tests/diag_test.rs", Rule::AssertWithoutMessage, 10),
    ("tests/differential_test.rs", Rule::AssertWithoutMessage, 3),
    ("tests/docs_drift_test.rs", Rule::AssertWithoutMessage, 1),
    ("tests/e2e_test.rs", Rule::AssertWithoutMessage, 3),
    ("tests/editors_doc_test.rs", Rule::AssertWithoutMessage, 1),
    ("tests/features_e2e_test.rs", Rule::AssertWithoutMessage, 3),
    ("tests/fragments_e2e_test.rs", Rule::AssertWithoutMessage, 2),
    (
        "tests/grammar_reconcile_test.rs",
        Rule::AssertWithoutMessage,
        2,
    ),
    (
        "tests/include_rename_test.rs",
        Rule::AssertWithoutMessage,
        2,
    ),
    ("tests/includes_e2e_test.rs", Rule::AssertWithoutMessage, 2),
    (
        "tests/inlay_hints_e2e_test.rs",
        Rule::AssertWithoutMessage,
        1,
    ),
    ("tests/logic_test.rs", Rule::AssertWithoutMessage, 81),
    ("tests/module_readme_test.rs", Rule::AssertWithoutMessage, 8),
    (
        "tests/navigation_e2e_test.rs",
        Rule::AssertWithoutMessage,
        2,
    ),
    ("tests/pii_guard_test.rs", Rule::AssertWithoutMessage, 10),
    ("tests/precedence_test.rs", Rule::AssertWithoutMessage, 1),
    (
        "tests/pull_diagnostics_e2e_test.rs",
        Rule::AssertWithoutMessage,
        2,
    ),
    ("tests/refactor_test.rs", Rule::AssertWithoutMessage, 6),
    (
        "tests/release_workflow_test.rs",
        Rule::AssertWithoutMessage,
        8,
    ),
    ("tests/rename_test.rs", Rule::AssertWithoutMessage, 4),
    ("tests/scaling_test.rs", Rule::AssertWithoutMessage, 2),
    ("tests/scratch_test.rs", Rule::AssertWithoutMessage, 3),
    ("tests/scratch_test.rs", Rule::CatchUnwind, 2),
    (
        "tests/versioned_catalog_test.rs",
        Rule::AssertWithoutMessage,
        2,
    ),
    (
        "tests/watched_files_e2e_test.rs",
        Rule::AssertWithoutMessage,
        1,
    ),
    ("tests/wiki_build_test.rs", Rule::AssertWithoutMessage, 4),
];

/// Whether a file must deny `unwrap()`: it is a crate root, and not a test.
fn unwrap_rule(shown: &str) -> UnwrapRule {
    let root = shown == "src/lib.rs"
        || shown == "src/main.rs"
        || (shown.starts_with("examples/") && shown.matches('/').count() == 1)
        || shown.starts_with("fuzz/fuzz_targets/");
    if root {
        UnwrapRule::MustDeny
    } else {
        UnwrapRule::NotApplicable
    }
}

#[test]
fn every_rust_file_follows_the_rules_lints_cannot_see() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fns = function_index(root);
    assert!(
        fns.len() > 100,
        "only {} functions indexed; the index is not seeing src/",
        fns.len()
    );
    let files = rust_files(root);
    assert!(
        files.len() > 50,
        "only {} Rust files found; the walk is not seeing the tree",
        files.len()
    );
    let mut listed = Vec::new();
    let mut counts: BTreeMap<(String, Rule), usize> = BTreeMap::new();
    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{} is not readable: {e}", path.display()));
        let name = path.file_name().and_then(|n| n.to_str());
        let kind = match name {
            Some("lib.rs" | "mod.rs") => ModuleKind::NonLeaf,
            _ => ModuleKind::Leaf,
        };
        let shown = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        let found = check_source(&source, kind, unwrap_rule(&shown), &fns)
            .unwrap_or_else(|e| panic!("{shown} does not parse: {e}"));
        for f in found {
            listed.push(format!("{shown}:{} {:?}", f.line, f.rule));
            *counts.entry((shown.clone(), f.rule)).or_default() += 1;
        }
    }
    let allowed: BTreeMap<(String, Rule), usize> = PENDING
        .iter()
        .map(|(file, rule, n)| (((*file).to_owned(), *rule), *n))
        .collect();
    let keys: BTreeSet<&(String, Rule)> = counts.keys().chain(allowed.keys()).collect();
    let mut wrong = Vec::new();
    for key in keys {
        let found = counts.get(key).copied().unwrap_or(0);
        let pending = allowed.get(key).copied().unwrap_or(0);
        if found != pending {
            wrong.push(format!(
                "{} {:?}: {found} found, PENDING holds {pending}",
                key.0, key.1
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the findings differ from PENDING:\n  {}\nall {} findings:\n  {}",
        wrong.join("\n  "),
        listed.len(),
        listed.join("\n  ")
    );
}
