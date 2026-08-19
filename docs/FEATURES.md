# Features, Settings & Snippets

## Features

#### Diagnostics

Your `opensips.cfg` is checked by the **real OpenSIPS parser**
(`opensips -C`) on open and save; errors appear as squiggles at the
exact line and column the parser reports. Failed or timed-out checks
clear stale squiggles instead of leaving them pinned; results are
versioned against the buffer they were computed for; runs are
serialized, time-bounded, and output-capped. Diagnostics are
automatically off in untrusted workspaces (checking a config loads
its modules, which executes code).

#### Completion

Context-sensitive, from documentation harvested out of your OpenSIPS
source tree (result cached per tree):

| You type | You get |
|---|---|
| `loadmodule "` | every module in the tree |
| `modparam("` | module names |
| `modparam("tm", "` | tm's parameters, each with docs |
| letters in a route | exported functions of **loaded** modules, core functions, core parameters, route names, keywords |
| `$` | pseudo-variables with descriptions |

Function completions insert **snippets** — the cursor lands between
the parentheses (`t_relay(│)`) — disable with
`opensipsLsp.completion.snippets`.

#### Hover, navigation, outline

Hover any function/parameter/module/`$variable` for its
documentation; **Ctrl+Click** a `route(name)` reference to jump to
its definition; **Ctrl+Shift+O** lists every route block.

#### Static snippets

Type a prefix and press Tab: `route`, `routen`, `failure_route`,
`onreply_route`, `branch_route`, `event_route`, `startup_route`,
`loadmodule`, `modparam`, `ifmethod`, `ifelse`, `while`, `switch`,
`xlog`, `send_reply`.

## Settings

VS Code settings (Ctrl+, → search "opensips"); other editors pass the
initialization option; environment variables are the fallback for
clients that can't pass options.

| VS Code setting | Init option | Environment | Default | Effect |
|---|---|---|---|---|
| `opensipsLsp.enable` | — | — | `true` | Master switch for the extension. |
| `opensipsLsp.serverPath` | — | — | `opensips-lsp` | Server binary; the default uses the copy bundled in platform builds, then PATH. |
| `opensipsLsp.opensipsPath` | `opensipsPath` | `OPENSIPS_LSP_BIN` | `opensips` | Binary for `-C` diagnostics; empty disables them. |
| `opensipsLsp.opensipsSrc` | `opensipsSrc` | `OPENSIPS_LSP_SRC` | *(unset)* | Source tree for completion/hover docs. |
| `opensipsLsp.diagnostics.enable` | *(maps to empty `opensipsPath`)* | — | `true` | Toggle diagnostics without losing the configured path. |
| `opensipsLsp.diagnostics.maxProblems` | `maxDiagnostics` | — | `100` | Bound on published diagnostics per file. |
| `opensipsLsp.checkTimeoutMs` | `checkTimeoutMs` | `OPENSIPS_LSP_CHECK_TIMEOUT_MS` | `10000` | Kill a `-C` run after this many ms. |
| `opensipsLsp.completion.snippets` | `snippetCompletions` | — | `true` | Function completions as tabstop snippets. |
| `opensipsLsp.cacheDir` | `cacheDir` | `OPENSIPS_LSP_CACHE_DIR` | platform cache dir | Documentation-catalog cache location. |
| `opensipsLsp.trace.server` | — | — | `off` | LSP traffic tracing in the output channel. |
| — | — | `OPENSIPS_LSP_OUTPUT_CAP_BYTES` | `1048576` | Byte cap on captured `-C` output. |

## Notes

- Server-backed options apply at initialization; the VS Code client
  restarts the server automatically when any `opensipsLsp.*` setting
  changes, so edits take effect immediately.
- Snippet completions and static snippets compose: static snippets
  scaffold blocks, completion snippets fill in calls.
