# opensips-lsp

A Language Server Protocol implementation for the OpenSIPS routing
script language (`opensips.cfg`).

## What it does

| Feature | How |
|---|---|
| **Diagnostics** | Runs `opensips -C -f <file>` on open/save and maps its parse errors (file:line:col) to LSP diagnostics — full-fidelity, version-exact semantic validation by the real parser. A fast analyzer layer warns between saves (debounced on change): undefined `route()` targets and duplicate route definitions. |
| **Completion** | Context-sensitive: module names after `loadmodule "` / `modparam("`, the module's parameters inside the second `modparam` argument, exported functions of *loaded* modules plus core functions/parameters, route names inside `route(` and in route bodies, keywords, and pseudo-variables after `$` (replacing the typed token). Duplicate labels collapse; `include_file`/`import_file` closures count. |
| **Hover** | Documentation for module functions, parameters, and modules, harvested from the OpenSIPS docs. |
| **Signature help** | The innermost unclosed call's signature with the active parameter, on `(` and `,`. |
| **Go to definition** | `route(name)` references resolve to their `route[name]` block — in this file or any included file. |
| **References / rename / highlights** | Every call site + definition of a route name; rename rewrites them all (charset-gated, quoted call sites handled). `prepareRename` blocks F2 off-symbol and pre-selects the name. |
| **Document symbols** | All route blocks (`route`, `failure_route`, `onreply_route`, …) with full block extents, nested outline. |
| **Folding** | Route-family blocks fold; brace matching is string/comment-safe. |
| **Workspace symbols** | Ctrl+T route search across open files and includes. |
| **Code lenses** | Reference counts above route definitions (include-closure-wide). |
| **Quick fixes** | Load the module exporting an unknown function; create a missing route stub. |
| **Catalog-pinned validation** | Undocumented `modparam` parameters flagged against YOUR source tree's docs. |
| **Include links** | `include_file`/`import_file` paths are Ctrl+Click document links. |
| **Formatting** | Whole-document and range formatting re-indents by brace depth and strips trailing whitespace, honouring the editor's tab settings. Line-preserving: it never joins, splits or reorders lines, and never touches a string or comment body. |
| **Semantic tokens** | Route names and pseudo-variables colored by analysis; editors that ask for a `semanticTokens/range` get exactly the tokens in view. |
| **Live settings** | Runtime toggles (analyzer, snippets, code lens, max diagnostics, check timeout) apply over `workspace/didChangeConfiguration` without a server restart. |
| **CLI** | `opensips-lsp check [--strict] [--bin <opensips>] <file>...` for CI/git hooks. |

Positions are exchanged in UTF-16 units (the LSP default) and are
correct on multibyte lines; doc harvests are cached per source tree
(see the admin guide's Caching section).

The documentation catalog is harvested at startup from an OpenSIPS
source tree. The 4.x markdown docs (`modules/*/README.md`) are the
most current and win; docbook (`modules/*/doc/*_admin.xml`) is the
fallback for older trees or placeholder READMEs. Core-language docs
(functions, parameters, pseudo-variables) come from `docs/manual/`.

Supported and version-proven: **OpenSIPS 4.0.x** (4.0.1) and
**3.6.x** (3.6.8) — the proof suite runs against a real tree and
binary of each (`OPENSIPS_LSP_TEST_TREE`/`OPENSIPS_LSP_TEST_BIN`),
built from the tag rather than from a moving branch.  Nothing pins a
version: module and core docs come from the tree you point at, and
diagnostics from your own binary.

## Configuration

Via LSP `initializationOptions` (or environment fallback):

| Option | Env | Default | Meaning |
|---|---|---|---|
| `opensipsPath` | `OPENSIPS_LSP_BIN` | `opensips` | Binary used for `-C` diagnostics. |
| `opensipsSrc` | `OPENSIPS_LSP_SRC` | *(none)* | Source tree to harvest module docs from. |

Diagnostics fidelity note: `-C` loads the modules the cfg references,
so it needs a tree/installation where those `.so` files exist (an
unresolvable module is itself reported as a diagnostic, which is
usually what you want).

## Install

**New to all of this?** Follow the
[Getting Started guide](docs/GETTING_STARTED.md) — one-command
install plus click-by-click usage instructions. Short version:

```sh
curl -fsSL https://raw.githubusercontent.com/NormB/opensips-lsp/main/install.sh | sh
```


Prebuilt server binaries (Linux, macOS, and Windows (x86_64 and arm64)) and the VS Code
`.vsix` ship with every [GitHub release](https://github.com/NormB/opensips-lsp/releases):

```sh
tar xzf opensips-lsp-<version>-x86_64-linux-gnu.tar.gz
install -m755 opensips-lsp ~/.local/bin/
```

## Build & test

```sh
cargo build --release        # server binary: target/release/opensips-lsp
cargo test                   # full suite, includes a stdio LSP e2e test
```

## Tree-sitter grammar

`tree-sitter-opensips/` carries an error-tolerant grammar for editors
that highlight and fold via tree-sitter (Neovim, Helix, Zed): corpus
tests run in CI; `tree-sitter generate` builds the parser locally.

## Documentation

- [`docs/FEATURES.md`](docs/FEATURES.md) — every feature, every
  setting (VS Code / init option / environment), and the snippet set.
- [`docs/ADMIN.md`](docs/ADMIN.md) — admin guide in the OpenSIPS
  module-doc structure (overview, dependencies, exported parameters,
  security, FAQ). Its structure is itself validated by the test suite
  through this project's own OpenSIPS-README harvester.
- [`docs/EDITORS.md`](docs/EDITORS.md) — setup for VS Code, Neovim,
  Helix, Emacs, Vim, Sublime Text, and Kate.
- API docs: `cargo doc --open` (`missing_docs` is `deny`).

## Editors

- **VS Code**: the `client/` directory contains the extension
  (`npm install && npm run compile`, then run/package with vsce).
  Settings: `opensipsLsp.serverPath`, `opensipsLsp.opensipsPath`,
  `opensipsLsp.opensipsSrc`.
- **Neovim** (0.10+):

  ```lua
  vim.api.nvim_create_autocmd("FileType", {
    pattern = "opensips-cfg",
    callback = function()
      vim.lsp.start({
        name = "opensips-lsp",
        cmd = { "opensips-lsp" },
        init_options = {
          opensipsPath = "/usr/local/sbin/opensips",
          opensipsSrc = "/path/to/opensips",
        },
      })
    end,
  })
  ```

## Design

- `src/catalog.rs` — docbook + markdown documentation harvester
- `src/analyze.rs` — comment/string-aware lexical scan of cfg text
  (loadmodules, routes, cursor context); deliberately *not* a grammar
- `src/diag.rs` — `opensips -C` output parser
- `src/logic.rs` — pure completion/hover/definition assembly
- `src/server.rs` — tower-lsp-server wiring

Semantic truth stays in OpenSIPS itself (`-C`); the server never
guesses about grammar validity, so it is automatically correct for
whatever OpenSIPS version it is pointed at.

## Security note

`opensips -C` **dlopens the modules the cfg loads** — their
constructors run. Opening a config from an untrusted source therefore
executes code paths you did not write. Rely on your editor's
workspace-trust prompt, and/or disable diagnostics entirely by
setting `opensipsPath` (or `OPENSIPS_LSP_BIN`) to an **empty string**
— completion, hover, and navigation keep working without it.
`-C` runs are serialized and bounded (10s default,
`OPENSIPS_LSP_CHECK_TIMEOUT_MS` to tune).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any
contribution intentionally submitted for inclusion in the work by
you, as defined in the Apache-2.0 license, shall be dual licensed as
above, without any additional terms or conditions.
