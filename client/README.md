# OpenSIPS Routing Script

Language support for the [OpenSIPS](https://opensips.org) routing
script (`opensips.cfg`) — **the real OpenSIPS parser checks your
config as you work**, and completion, hover documentation, and
navigation come from the actual documentation of your OpenSIPS
version. Platform builds bundle the language server: install the
extension and it just works.

## Features

- **Diagnostics you can trust** — every save runs `opensips -C`, so
  the squiggles are the *real* parser's verdict, at the exact line
  and column, for exactly your OpenSIPS version:
  `Parameter <fr_timeot> not found in module <tm> - can't set`.
- **Completion that knows context**
  - `loadmodule "` → every module in your source tree
  - `modparam("tm", "` → tm's parameters, with their documentation
  - inside a route → exported functions of the modules you loaded,
    plus core functions, parameters, route names, and keywords
  - `$` → pseudo-variables (`$ru`, `$si`, …) with descriptions
  - functions insert as snippets — the cursor lands between the
    parentheses
- **Hover documentation** for functions, parameters, modules, and
  pseudo-variables, harvested from OpenSIPS's own docs.
- **Navigation** — Ctrl+Click `route(name)` to jump to its
  definition (even into an `include_file`); Ctrl+Shift+O lists every
  route block with its full extent; route blocks fold.
- **Signature help** — type `(` or `,` in a call and the signature
  pops up with the active parameter highlighted.
- **References & rename** — Shift+F12 lists every call site of a
  route; F2 renames it everywhere, quoted call sites included.
- **Instant warnings** — undefined `route()` targets, duplicate
  route definitions, and modparams your OpenSIPS version doesn't
  document are flagged as you type, no save needed.
- **Quick fixes** — the lightbulb loads the module that exports an
  unknown function, or creates a missing route stub.
- **Workspace symbols & code lenses** — Ctrl+T finds any route;
  reference counts appear above route definitions.
- **Semantic highlighting** — route names and pseudo-variables are
  colored by real analysis, even inside strings.
- **Snippets** — `route`, `failure_route`, `ifmethod`, `modparam`,
  `switch`, `xlog`, and more.
- **Safe by default** — in untrusted workspaces diagnostics stay off
  (checking a config loads its modules); everything else keeps
  working.

## Quick start

1. Install this extension (the platform packages bundle the server —
   Linux, macOS, and Windows, x64 and arm64).
2. Open a folder containing an `opensips.cfg` — syntax colors,
   completion, and navigation work immediately. Files named
   `opensips.cfg` or `*.opensips.cfg` are recognized automatically;
   for other names (split configs, includes) add a
   [`files.associations`](https://code.visualstudio.com/docs/languages/identifiers)
   entry mapping them to `opensips-cfg` — the generic `.cfg`
   extension is deliberately not claimed, so unrelated tools' config
   files are left alone.
3. For live error checking, point
   **Settings → Opensips Lsp: Opensips Path** at your `opensips`
   binary and save the file.
4. For the richest completion docs, set **Opensips Src** to an
   OpenSIPS source tree matching your version.

New to all of this? The step-by-step
[Getting Started guide](https://github.com/NormB/opensips-lsp/blob/main/docs/GETTING_STARTED.md)
covers installation and usage click by click.

## Settings

| Setting | Default | Effect |
|---|---|---|
| `opensipsLsp.enable` | `true` | Master switch. |
| `opensipsLsp.serverPath` | bundled | Server binary override. |
| `opensipsLsp.opensipsPath` | `opensips` | Binary for `-C` diagnostics; empty disables. |
| `opensipsLsp.opensipsSrc` | — | Source tree for completion/hover docs. |
| `opensipsLsp.diagnostics.enable` | `true` | Toggle checks without losing the path. |
| `opensipsLsp.diagnostics.maxProblems` | `100` | Diagnostics cap per file. |
| `opensipsLsp.diagnostics.analyzer` | `true` | As-you-type analyzer warnings. |
| `opensipsLsp.codeLens.references` | `true` | Reference-count code lenses. |
| `opensipsLsp.checkTimeoutMs` | `10000` | Bound on one `-C` run. |
| `opensipsLsp.completion.snippets` | `true` | Function completions as snippets. |
| `opensipsLsp.cacheDir` | platform | Documentation-cache location. |
| `opensipsLsp.trace.server` | `off` | LSP traffic tracing. |

Full reference:
[Features & Settings](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md).

## Requirements

None to start — the server is bundled. Optional, for the full
experience: an `opensips` binary (diagnostics) and an OpenSIPS source
tree (documentation). Supports OpenSIPS 4.x and 3.6.x.

## Links

[Repository](https://github.com/NormB/opensips-lsp) ·
[Getting Started](https://github.com/NormB/opensips-lsp/blob/main/docs/GETTING_STARTED.md) ·
[Admin Guide](https://github.com/NormB/opensips-lsp/blob/main/docs/ADMIN.md) ·
[Issues](https://github.com/NormB/opensips-lsp/issues) ·
[Releases](https://github.com/NormB/opensips-lsp/releases)

Dual-licensed MIT or Apache-2.0.
