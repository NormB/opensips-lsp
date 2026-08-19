# OpenSIPS Language Support

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
  `Parameter <fr_timeot> not found in module <tm>`.
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
  definition; Ctrl+Shift+O lists every route block.
- **Snippets** — `route`, `failure_route`, `ifmethod`, `modparam`,
  `switch`, `xlog`, and more.
- **Safe by default** — in untrusted workspaces diagnostics stay off
  (checking a config loads its modules); everything else keeps
  working.

## Quick start

1. Install this extension (the platform packages bundle the server —
   Linux, macOS, and Windows, x64 and arm64).
2. Open a folder containing an `opensips.cfg` — syntax colors,
   completion, and navigation work immediately.
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
