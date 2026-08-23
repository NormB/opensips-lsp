# Features, Settings & Snippets

## Features

#### Diagnostics

Two complementary layers:

- **Parser diagnostics** — your `opensips.cfg` is checked by the
  **real OpenSIPS parser** (`opensips -C`) on open and save; errors
  appear as squiggles at the exact line and column the parser
  reports. Failed or timed-out checks clear stale squiggles instead
  of leaving them pinned; results are versioned against the buffer
  they were computed for; runs are serialized, time-bounded, and
  output-capped. Automatically off in untrusted workspaces (checking
  a config loads its modules, which executes code).
- **Analyzer warnings** — fast, between saves, as you type
  (debounced): `route(name)` calls whose target is defined nowhere in
  the file or its includes, and duplicate route definitions. Source
  `opensips-lsp`, severity warning; toggle with
  `opensipsLsp.diagnostics.analyzer`.

#### Completion

Context-sensitive, from documentation harvested out of your OpenSIPS
source tree (result cached per tree):

| You type | You get |
|---|---|
| `loadmodule "` | every module in the tree |
| `modparam("` | module names |
| `modparam("tm", "` | tm's parameters, each with docs |
| letters in a route | exported functions of **loaded** modules, core functions, core parameters, route names, keywords |
| `route(` | route names (this file and its includes) |
| `$` | pseudo-variables with descriptions (the typed `$word` is replaced, never doubled) |

Duplicate labels are collapsed, keeping the most informative item.
Modules loaded and routes defined in `include_file`/`import_file`
files count — the closure is followed (open editor buffers preferred
over disk).

Function completions insert **snippets** — the cursor lands between
the parentheses (`t_relay(│)`) — disable with
`opensipsLsp.completion.snippets`.

#### Signature help

Type `(` or `,` inside a call and the function's signature pops up
with the active parameter highlighted — module exports first, then
core functions. Commas inside strings don't advance the parameter.

#### Hover, go to definition, document symbols

Hover any function/parameter/module/`$variable` for its
documentation; **Ctrl+Click** a `route(name)` reference to jump to
its definition — including definitions that live in an included
file; **Ctrl+Shift+O** lists every route block with its full extent
(the outline nests, and breadcrumbs know which block you're in).

#### References, rename, highlights

**Shift+F12** lists every call site and the definition of a route
name; **F2** renames a route everywhere (quoted call sites are
rewritten inside the quotes; illegal names are rejected); all
occurrences of the route under the cursor are highlighted, the
definition as a write. **F2** first asks the server
(`prepareRename`) whether the cursor is on a renameable route symbol,
so off-symbol renames are blocked up front with the symbol
pre-selected otherwise.

Route families are separate namespaces, matching OpenSIPS: `route(x)`
invokes only the main table (`route[x]`), so navigation, references,
and rename never cross into a same-named `failure_route[x]`,
`event_route[x]`, etc. — renaming one leaves the others untouched.

#### Workspace symbols, code lenses

**Ctrl+T** searches route definitions across every open file and its
includes. Named `route` blocks show a **reference count** code lens
(counted across the include closure); disable with
`opensipsLsp.codeLens.references`.

#### Call hierarchy

**Shift+Alt+H** on a route name — at a `route(NAME)` call or on the
`route[NAME]` definition — opens the call graph. Incoming calls list
every block that calls it; outgoing calls list every route it calls.
Both span the include closure, so a caller living in an included file
shows up with that file's URI.

Several calls from the same block collapse into one entry carrying
each call site's range, so the editor can step through them.

A route called but defined nowhere still appears as an outgoing edge,
marked `undefined` — the call is in the file, and dropping it would
hide something the reader can see.

The graph is the **main route table**. `route(NAME)` is the only call
form the server can observe, so `route[NAME]` blocks are what take
part. A `failure_route[NAME]` or `event_route[NAME]` is armed by a
module function that takes the route name as a string
(`t_on_failure("NAME")`), which the server does not track: those
blocks can *make* calls, and do show up as callers, but asking for
their own hierarchy declines rather than reporting "no callers" —
which would be a confident wrong answer.

#### Quick fixes

The lightbulb offers: **Load module 'X'** when the parser reports
`unknown command <f>` and the catalog knows which module exports
`f` (inserted after the last `loadmodule`), and **Create route[x]**
for an undefined `route(x)` target (a stub is appended).

#### Catalog-pinned validation

`modparam("m", "p", ...)` warns as you type when the configured
source tree documents module `m` but no parameter `p` — version-exact
by construction, since the catalog IS your pinned tree. Unknown
modules stay silent.

#### Include links

`include_file`/`import_file` paths are document links: **Ctrl+Click**
opens the included file (relative paths resolve against the including
file's directory; links are produced even for not-yet-created files).

#### Semantic highlighting

Route names (definitions and call sites) and pseudo-variables get
semantic tokens, so themes color them consistently — including pvars
inside strings (either quote style), where OpenSIPS interpolates
them. Comments — line or block — are excluded byte-by-byte through
the same classifier the analyzer uses, so a `#` inside a string does
not hide the rest of the line and a `/* ... */` block hides all of
its interior.

Editors that issue a `semanticTokens/range` request (large files,
visible-viewport optimization) get exactly the tokens inside it.

#### Formatting

**Shift+Alt+F** (or format-on-save) re-indents the document by brace
depth and strips trailing whitespace. Selecting a region and using
range formatting does the same for those lines only, at the
indentation a whole-document pass would have given them.

The formatter is deliberately **line-preserving**: it rewrites the
leading and trailing whitespace of a line and nothing else. It never
joins, splits or reorders lines, never touches a byte inside a string
literal or a comment body, and never emits an edit for a line that is
already correct — so folding, selection and cursor position survive.
Braces inside strings and comments do not move the indent depth.

Two things it will not touch:

- **Continuation lines of a multi-line string or block comment** —
  their leading whitespace is content, not layout.
- **`#!` preprocessor directives** — these are processed line-wise
  ahead of the parser, so their column is not the formatter's to move.

Indentation follows the editor: the `insertSpaces` and `tabSize` the
client sends with the request decide tabs versus spaces and the width.
Upstream `etc/opensips.cfg` is tab-indented, which is what a client
sending no preference gets.

The guarantee is tested three ways: the reformatted document must be
identical to the original once leading and trailing whitespace is
stripped from every line; formatting must be idempotent; and, against
a real binary, the positioned parse errors `opensips -C` reports must
be unchanged by formatting.

#### CLI check mode

`opensips-lsp check [--strict] [--bin <opensips>] <file>...` runs the
same analyzer (plus the real `-C` when a binary is given) for CI
pipelines and git hooks. Exit codes: 0 clean, 1 findings, 2 usage.

#### Folding

Every route-family block folds (`route`, `failure_route[x]`,
`event_route[...]`, ...), with string- and comment-safe brace
matching.

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
| `opensipsLsp.diagnostics.analyzer` | `analyzerDiagnostics` | — | `true` | Fast analyzer warnings between saves (undefined `route()` targets, duplicate definitions, undocumented modparams). |
| `opensipsLsp.codeLens.references` | `codeLensReferences` | — | `true` | Reference-count code lenses on route definitions. |
| `opensipsLsp.checkTimeoutMs` | `checkTimeoutMs` | `OPENSIPS_LSP_CHECK_TIMEOUT_MS` | `10000` | Kill a `-C` run after this many ms. |
| `opensipsLsp.completion.snippets` | `snippetCompletions` | — | `true` | Function completions as tabstop snippets. |
| `opensipsLsp.cacheDir` | `cacheDir` | `OPENSIPS_LSP_CACHE_DIR` | platform cache dir | Documentation-catalog cache location. |
| `opensipsLsp.trace.server` | — | — | `off` | LSP traffic tracing in the output channel. |
| — | — | `OPENSIPS_LSP_OUTPUT_CAP_BYTES` | `1048576` | Byte cap on captured `-C` output. |

## Notes

- Runtime toggles (`diagnostics.analyzer`, `completion.snippets`,
  `codeLens.references`, `diagnostics.maxProblems`, `checkTimeoutMs`)
  apply **live**: the client pushes them to the running server over
  `workspace/didChangeConfiguration` and open documents republish
  immediately — no restart. Settings that shape initialization
  (`serverPath`, `opensipsPath`, `opensipsSrc`, `cacheDir`, `enable`,
  `diagnostics.enable`) still restart the server automatically.
- Snippet completions and static snippets compose: static snippets
  scaffold blocks, completion snippets fill in calls.
- Include handling is capped for safety: depth 8, 64 files, 1 MiB
  per file (OpenSIPS itself allows depth 50). The LSP resolves
  relative include paths against the including file's directory;
  note that at runtime OpenSIPS tries the process working directory
  FIRST and only then the including file's directory — a same-named
  file in the daemon's CWD can differ from what the editor analyzed.
  `OPENSIPS_LSP_ANALYZER_DEBOUNCE_MS` tunes the analyzer debounce
  (default 300).
- The analyzer's undefined-route warnings are ADDITIVE to
  `opensips -C`: the parser accepts a config whose `route(x)` target
  does not exist (the failure happens at runtime), so only the
  analyzer catches it while editing.
