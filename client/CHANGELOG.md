# Changelog

All notable changes to the OpenSIPS Configuration extension.

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
