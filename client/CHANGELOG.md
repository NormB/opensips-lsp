# Changelog

All notable changes to the OpenSIPS Routing Script extension.

## [0.6.1] — 2026-08-20

Deep-audit release: every claim was re-verified against the real
OpenSIPS binary and grammar (cfg.y/cfg.lex), and everything found was
fixed.

- **Diagnostics ranges corrected**: OpenSIPS reports an EXCLUSIVE end
  column; squiggles no longer extend one character past the token.
- **Errors inside `include_file` targets now surface** on the root
  document at the include directive (they were silently dropped — a
  broken config could render clean).
- **`timer_route[name, interval]` recognized** everywhere (outline,
  folding, duplicate detection, highlighting).
- **Single-quoted strings** (real OpenSIPS syntax) understood by the
  analyzer, both grammars, and route names/includes.
- **Rename is grammar-safe**: only names legal UNQUOTED (identifier
  or number) are accepted — renaming to `a.b`/`x-y` produced configs
  the parser rejects.
- **Analyzer precision**: `route(0)` resolves to the main route;
  `failure_route[x]` no longer satisfies `route(x)` (separate
  namespaces); duplicates tracked per kind. These warnings are
  additive — `opensips -C` accepts undefined route targets.
- **Signature help** keeps nested parameters intact
  (`json_link($json(a), $json(b))`).
- Keyword completion: `strlen` (nonexistent), `send_reply`
  (signaling module), `subst` (textops) removed; `break` added.
- **Security**: `serverPath` is now restricted in untrusted
  workspaces, like the checker path.
- References/rename operate across the include closure; docs quote
  the full parser message and state include-resolution semantics
  honestly; admin guide documents every option and env var
  (CI-gated).

## [0.6.0] — 2026-08-19

- **Signature help**: the innermost unclosed call's signature with
  the active parameter highlighted, on `(` and `,` — module exports
  first, then core functions.
- **References, rename, highlights** for route names: Shift+F12
  lists every call site and the definition, F2 renames everywhere
  (quoted call sites rewritten inside the quotes, illegal names
  rejected), occurrences highlight with the definition as a write.
- **Include awareness**: `include_file`/`import_file` are followed
  (open buffers preferred over disk, cycle-safe, capped) — completion
  sees included modules and routes, and go-to-definition jumps into
  the include. Dotted route names resolve whole.
- **Instant analyzer warnings** between saves, debounced as you
  type: undefined `route()` targets and duplicate route definitions.
  New setting `opensipsLsp.diagnostics.analyzer`.
- **Folding** for every route-family block, and the outline/breadcrumbs
  now carry full block extents (nested document symbols).
- Completion quality: duplicate labels collapse to the most
  informative item, a typed `$token` is replaced instead of doubled,
  and `route(` completes route names.
- Fixed: `maxDiagnostics` was ignored in the check path (shadowed by
  the output cap).

## [0.5.4] — 2026-08-19

- Display name is now "OpenSIPS Routing Script" (the previous names
  collide with Microsoft's marketplace similarity rules).

## [0.5.3] — 2026-08-19

- Display name is now "OpenSIPS Language Support".

## [0.5.2] — 2026-08-19

- Full changelog now ships with the extension (this document).
- Rewritten overview page.

## [0.5.1] — 2026-08-19

- The OpenSIPS logo mark is now the extension icon.

## [0.5.0] — 2026-08-19

- **Snippets**: function completions insert tabstop snippets
  (`t_relay(│)`), and 15 static snippets scaffold route blocks,
  `loadmodule`/`modparam`, control flow, `xlog`, and `send_reply`.
- **New settings**: `opensipsLsp.enable`, `opensipsLsp.trace.server`,
  `opensipsLsp.diagnostics.enable`,
  `opensipsLsp.diagnostics.maxProblems`,
  `opensipsLsp.completion.snippets`, `opensipsLsp.cacheDir`.
- New docs page: Features & Settings — every feature, every setting,
  every environment fallback.

## [0.4.1] — 2026-08-19

- Description cleanup.

## [0.4.0] — 2026-08-19

- **Windows support**: native server builds for Windows x64 and
  arm64, `win32-x64`/`win32-arm64` extension packages with the server
  bundled, and a PowerShell one-command installer (`install.ps1`).
- Full platform matrix: Linux, macOS, and Windows, x64 and arm64
  each — the editor picks its platform package automatically.

## [0.3.4] — 2026-08-19

- Open VSX publishing is automated on every release.

## [0.3.3] — 2026-08-19

- Extension identity finalized as `NormB.opensips-lsp`.
- macOS server builds (arm64 native, x64 cross-compiled).

## [0.3.0] — 2026-08-19

- **Bundled server**: platform extension packages carry the
  `opensips-lsp` server inside — install the extension and it just
  works; no separate download.
- Workspace-trust gate: diagnostics stay off in untrusted folders
  (checking a config loads its modules); settings changes restart the
  server live.
- Hardening: failed or timed-out checks clear stale squiggles,
  diagnostics are versioned against the buffer they were computed
  for, checker output and message lengths are bounded, diagnostics
  attach by canonical path (symlink-safe), harvested documentation is
  sanitized before rendering.
- One-command installer and the novice Getting Started guide.
- Corpus-proven grammar: every syntactically valid example config
  shipped with OpenSIPS parses.

## [0.2.0] — 2026-08-19

- Correct positions on multibyte lines (UTF-16 mapping end to end).
- Documentation-catalog caching per source tree.
- Tree-sitter grammar for error-tolerant highlighting.
- Full OpenSIPS 4.x core-language coverage: core functions, core
  parameters, and pseudo-variable completion after `$`; proven
  against real 4.x and 3.6.x trees and binaries.

## [0.1.0] — 2026-08-19

- Initial release: diagnostics via `opensips -C`, context-sensitive
  completion (modules, parameters, functions of loaded modules),
  hover documentation, go-to-definition for routes, and document
  symbols.
