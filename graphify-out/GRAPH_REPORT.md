# Graph Report - opensips-lsp  (2026-08-19)

## Corpus Check
- 20 files · ~6,150 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 220 nodes · 328 edges · 15 communities
- Extraction: 91% EXTRACTED · 9% INFERRED · 0% AMBIGUOUS · INFERRED: 30 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `4ee1e3c2`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Backend
- package.json
- ModuleDoc
- analyze.rs
- logic.rs
- properties
- parse_check_output
- e2e_test.rs
- compilerOptions
- opensips-lsp
- opensips.tmLanguage.json

## God Nodes (most connected - your core abstractions)
1. `Backend` - 26 edges
2. `ModuleDoc` - 12 edges
3. `parse_admin_xml()` - 10 edges
4. `loaded_modules()` - 9 edges
5. `completions()` - 9 edges
6. `catalog()` - 9 edges
7. `route_defs()` - 8 edges
8. `route_refs()` - 8 edges
9. `parse_readme_md()` - 8 edges
10. `parse_check_output()` - 8 edges

## Surprising Connections (you probably didn't know these)
- `finds_loaded_modules_skipping_comments()` --calls--> `loaded_modules()`  [INFERRED]
  tests/analyze_test.rs → src/analyze.rs
- `loadmodule_inside_string_is_not_collected()` --calls--> `loaded_modules()`  [INFERRED]
  tests/analyze_test.rs → src/analyze.rs
- `loadmodule_positions_are_line_accurate()` --calls--> `loaded_modules()`  [INFERRED]
  tests/analyze_test.rs → src/analyze.rs
- `finds_route_definitions()` --calls--> `route_defs()`  [INFERRED]
  tests/analyze_test.rs → src/analyze.rs
- `finds_route_references()` --calls--> `route_refs()`  [INFERRED]
  tests/analyze_test.rs → src/analyze.rs

## Import Cycles
- None detected.

## Communities (15 total, 0 thin omitted)

### Community 0 - "Backend"
Cohesion: 0.07
Nodes (29): Client, CompletionParams, CompletionResponse, DashMap, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams (+21 more)

### Community 1 - "package.json"
Cohesion: 0.07
Nodes (26): activationEvents, categories, dependencies, vscode-languageclient, description, devDependencies, @types/node, @types/vscode (+18 more)

### Community 2 - "ModuleDoc"
Cohesion: 0.14
Nodes (19): Node, collapsed_text(), first_para(), harvest_tree(), Item, ModuleDoc, neutralize_entities(), parse_admin_xml() (+11 more)

### Community 3 - "analyze.rs"
Cohesion: 0.20
Nodes (19): Class, classify(), is_word(), line_col(), loaded_modules(), Located, modparam_context(), route_defs() (+11 more)

### Community 4 - "logic.rs"
Cohesion: 0.18
Nodes (20): Comp, CompKind, completions(), definition_of(), diag_matches_file(), hover_markdown(), resolve_bin(), Option (+12 more)

### Community 5 - "properties"
Cohesion: 0.11
Nodes (18): properties, title, contributes, configuration, grammars, languages, default, description (+10 more)

### Community 6 - "parse_check_output"
Cohesion: 0.24
Nodes (10): Diag, parse_check_output(), String, Vec, Severity, adversarial_output_does_not_panic(), column_range_never_reversed(), load_module_failure_uses_the_positioned_line() (+2 more)

### Community 7 - "e2e_test.rs"
Cohesion: 0.35
Nodes (11): Child, F, Receiver, diagnostics_flow_over_stdio(), empty_opensips_bin_disables_checks_entirely(), hanging_opensips_check_is_bounded_and_reported(), spawn_reader(), wait_for() (+3 more)

### Community 8 - "compilerOptions"
Cohesion: 0.20
Nodes (9): compilerOptions, module, outDir, rootDir, sourceMap, strict, target, include (+1 more)

### Community 9 - "opensips-lsp"
Cohesion: 0.25
Nodes (7): Build & test, Configuration, Design, Editors, opensips-lsp, Security note, What it does

### Community 10 - "opensips.tmLanguage.json"
Cohesion: 0.40
Nodes (4): name, patterns, $schema, scopeName

## Knowledge Gaps
- **46 isolated node(s):** `name`, `displayName`, `description`, `version`, `publisher` (+41 more)
  These have ≤1 connection - possible missing edges or undocumented components.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Backend` connect `Backend` to `ModuleDoc`?**
  _High betweenness centrality (0.228) - this node is a cross-community bridge._
- **Why does `ModuleDoc` connect `ModuleDoc` to `Backend`, `logic.rs`?**
  _High betweenness centrality (0.227) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `parse_admin_xml()` (e.g. with `backslashes_survive()` and `parses_params_and_functions()`) actually correct?**
  _`parse_admin_xml()` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Are the 4 inferred relationships involving `loaded_modules()` (e.g. with `adversarial_inputs_do_not_panic()` and `finds_loaded_modules_skipping_comments()`) actually correct?**
  _`loaded_modules()` has 4 INFERRED edges - model-reasoned connections that need verification._
- **Are the 5 inferred relationships involving `completions()` (e.g. with `adversarial_docs_do_not_panic()` and `code_position_offers_loaded_module_functions_and_routes()`) actually correct?**
  _`completions()` has 5 INFERRED edges - model-reasoned connections that need verification._
- **What connects `name`, `displayName`, `description` to the rest of the system?**
  _46 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Backend` be split into smaller, more focused modules?**
  _Cohesion score 0.06765327695560254 - nodes in this community are weakly interconnected._