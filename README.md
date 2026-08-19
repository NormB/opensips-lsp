# opensips-lsp

A Language Server Protocol implementation for the OpenSIPS routing
script language (`opensips.cfg`).

## What it does

| Feature | How |
|---|---|
| **Diagnostics** | Runs `opensips -C -f <file>` on open/save and maps its parse errors (file:line:col) to LSP diagnostics — full-fidelity, version-exact semantic validation by the real parser. |
| **Completion** | Context-sensitive: module names after `loadmodule "` / `modparam("`, the module's parameters inside the second `modparam` argument, exported functions of *loaded* modules plus route names and core keywords in route bodies. |
| **Hover** | Documentation for module functions, parameters, and modules, harvested from the OpenSIPS docs. |
| **Go to definition** | `route(name)` references resolve to their `route[name]` block. |
| **Document symbols** | All route blocks (`route`, `failure_route`, `onreply_route`, …). |

The documentation catalog is harvested at startup from an OpenSIPS
source tree: docbook (`modules/*/doc/*_admin.xml`) where present,
4.x markdown (`modules/*/README.md`) otherwise.

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

## Build & test

```sh
cargo build --release        # server binary: target/release/opensips-lsp
cargo test                   # full suite, includes a stdio LSP e2e test
```

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
- `src/server.rs` — tower-lsp wiring

Semantic truth stays in OpenSIPS itself (`-C`); the server never
guesses about grammar validity, so it is automatically correct for
whatever OpenSIPS version it is pointed at.
