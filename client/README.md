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
- **Navigation** — go to definition: Ctrl+Click `route(name)` to jump
  to its definition (even into an `include_file`); Ctrl+Shift+O lists every
  route block (document symbols) with its full extent; route blocks fold.
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
- **Formatting** — Shift+Alt+F or format-on-save re-indents by brace
  depth and strips trailing whitespace, following your editor's tab
  settings; range formatting does the same for a selection. Line-preserving on purpose: it never joins, splits or
  reorders lines, never touches a string or comment, and leaves a
  line that continues the previous statement exactly where you put
  it — a hanging indent or a braceless `if` body stays readable.
- **Call hierarchy** — Shift+Alt+H on a route name opens the route
  call graph: who calls `route[X]`, and what `route[X]` calls, across
  the include closure.
- **Inlay hints** — parameter names from the documentation drawn at
  each documented call site.
- **Include links** — Ctrl+Click an `include_file`/`import_file` path
  to open it.
- **Included files opened on their own** — open the folder (not the
  single file) and a fragment is answered in the context of the root
  that includes it. Given `opensips.cfg` with
  `include_file "routing/inbound.cfg"`, opening `routing/inbound.cfg`
  colors it, resolves and completes routes defined anywhere else in
  the configuration, stops calling them undefined, and runs
  `opensips -C` on the root — putting each error back on the file it
  actually names. Turn the coloring off with
  **Opensips Lsp › Associate Included Files**.
- **Pull diagnostics** — `textDocument/diagnostic` and a workspace
  sweep that reports problems across the project without opening
  every file. Only root configs are swept: a config another one
  includes is a fragment, not a program.
- **Watched files** — an include or the documentation tree changing
  on disk (a git checkout, a rebuild) re-checks and re-harvests
  without you touching the buffer.
- **Refactorings** — extract a selection into a `route[...]` of its
  own, leaving a call behind; remove duplicate `loadmodule` lines (a
  second load is a parse error, not untidiness). The extraction
  declines when it would change behaviour — a selection containing
  `return` is never lifted.
- **Live settings** — the runtime toggles apply without restarting
  the server.
- **Core language and every module, out of the box** — parameters,
  functions and pseudo-variables (`log_level`, `socket`, `mpath`) and
  the exported functions and parameters of 186 modules (`is_method`,
  `t_relay`, …) complete before you configure anything, from
  catalogues harvested from OpenSIPS 4.0.1 and shipped with the
  extension. Module functions still appear only in a config that
  loads the module. Hover says which version the docs came from;
  point `opensipsSrc` at your own source tree for docs exact to your
  build.
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
   `opensips.cfg`, `opensips*.cfg` (so `opensips-tls.cfg` and
   `opensips-local.cfg` work) or `*.opensips.cfg` are recognized
   automatically;
   the generic `.cfg` extension is deliberately not claimed, so
   unrelated tools' config files are left alone. A `.cfg` your
   configuration **includes** is picked up anyway: the extension asks
   the server what includes what and gives it the language, so a
   split-out `carrier-routes.cfg` gets the same colors and the same
   server as the root that pulls it in. A file another extension
   already claims is left to that extension, and for anything the
   includes do not reach, add a
   [`files.associations`](https://code.visualstudio.com/docs/languages/identifiers)
   entry mapping it to `opensips-cfg`.
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
tree (documentation). Supports OpenSIPS 4.0.x and 3.6.x.

## Privacy

Everything stays on your machine. The extension talks to a local
server over stdin/stdout: no HTTP client is linked into it, there is
no telemetry or analytics, and no model is involved — hover and
completion text is parsed from OpenSIPS's own documentation on disk.
The only program it runs is your own `opensips` binary, for `-C`
diagnostics. Your editor's own telemetry, and any AI extension you
have installed, are a separate matter from this one.

## Links

[Repository](https://github.com/NormB/opensips-lsp) ·
[Getting Started](https://github.com/NormB/opensips-lsp/blob/main/docs/GETTING_STARTED.md) ·
[Admin Guide](https://github.com/NormB/opensips-lsp/blob/main/docs/ADMIN.md) ·
[Issues](https://github.com/NormB/opensips-lsp/issues) ·
[Releases](https://github.com/NormB/opensips-lsp/releases)

Dual-licensed MIT or Apache-2.0.
