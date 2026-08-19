# opensips-lsp — LSP server for the OpenSIPS cfg language

Rust (`tower-lsp`) language server + VS Code client (`client/`).
Semantic truth = `opensips -C` subprocess; editor smarts from a doc
catalog harvested from an OpenSIPS source tree. All code is TDD:
red test first, then green — no exceptions (`cargo test`, includes a
stdio LSP e2e).

SECURITY: `opensips -C` dlopens the cfg's modules — code execution on
open. An explicit empty `opensipsPath`/`OPENSIPS_LSP_BIN` disables
diagnostics; see README "Security note".

## graphify

This project has a knowledge graph at graphify-out/ with god nodes,
community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when
  graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for
  relationships and `graphify explain "<concept>"` for focused
  concepts.
- If graphify-out/wiki/index.md exists, use it for broad navigation
  instead of raw source browsing.
- After modifying code, run `graphify update .` to keep the graph
  current (AST-only, no API cost).
