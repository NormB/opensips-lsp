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
  the file or its includes, duplicate route definitions, and `#!`
  lines that spell a Kamailio preprocessor directive — OpenSIPS has
  none, so `#!ifdef` guards nothing and `#!define` binds nothing.
  Source `opensips-lsp`, severity warning; toggle with
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

#### Inlay hints

Arguments at a documented call site are labelled with the parameter
name the module's own documentation gives them, so
`t_relay("udp", 1)` reads as `t_relay(flags: "udp", outbound_proxy: 1)`
without the document changing.

Only calls the catalogue knows are hinted. That is what keeps `if`,
`while` and `route` out of it without special-casing them: they are
not documented functions. A call carrying more arguments than the
signature documents is hinted as far as the signature goes and no
further — guessing past the end would be inventing names.

Signatures are written for humans (`[flags]`, `[outbound_proxy]`), so
the bracket markers, defaults and leading types are stripped down to
the name; a parameter that reduces to nothing is skipped rather than
drawn as an empty chip.

The editor asks for the visible range and only that range is
computed. Turn them off with `opensipsLsp.inlayHints.parameterNames`,
which applies live.

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

#### Refactorings

**Extract into a route.** Select statements inside a route body and
the lightbulb offers to lift them into a `route[EXTRACTED]` of their
own, leaving a `route(EXTRACTED);` call at the original indentation.
The new block lands after the enclosing one, and the generated name
steps aside from any name already in the file.

The action appears for a selection of whole lines, not for a bare
cursor or a word inside a line — it lifts *lines*, so offering it for
a sub-line selection would move more than was highlighted.

It declines more than it accepts, and each refusal is a case where
accepting would change what the config does:

- a selection outside a route body, or covering the block's own
  braces — there is nothing to lift, or lifting it would unbalance
  the file;
- unbalanced braces inside the selection, for the same reason;
- **a `return` in the selection.** `return` leaves the route it is
  written in. Moved into a new route it returns to the *caller*, so
  the statements after the extracted call would start running when
  they did not before. That is a behaviour change no editor should
  make silently, so the action is simply not offered.

Braces and `return` are judged in code position, so a `return` inside
a string or a comment does not block anything.

**Remove duplicate `loadmodule` lines.** A second `loadmodule` for the
same module is not untidiness — the real parser rejects it outright,
positioned on the second line. The action removes every occurrence
after the first and **does not reorder anything**: load order decides
module initialisation order and a `modparam` must follow its own
`loadmodule`, so sorting is not a transformation that can be applied
blind.

#### Catalog-pinned validation

`modparam("m", "p", ...)` warns as you type when the configured
source tree documents module `m` but no parameter `p` — version-exact
by construction, since the catalog IS your pinned tree. Unknown
modules stay silent.

#### Include links

`include_file`/`import_file` paths are document links: **Ctrl+Click**
opens the included file (relative paths resolve against the including
file's directory; links are produced even for not-yet-created files).

#### An included file opened on its own

A config another config includes is a **fragment**, not a program.
Checked on its own it flags every route its parent defines as
undefined and reports every construct it continues as a syntax error,
and `opensips -C` was never meant to be handed one. The workspace
sweep has always known this and skipped fragments — but opening one
directly used to produce exactly those errors, as artefacts of how
the file was opened rather than of anything wrong with it.

So a fragment is answered in its **root's** context. Given

```
/etc/opensips/
├── opensips.cfg          include_file "modules.cfg"
│                         include_file "routing/inbound.cfg"
├── modules.cfg
└── routing/
    ├── inbound.cfg       route[inbound] { route(send_to_carrier); }
    └── carriers.cfg      route[send_to_carrier] { … }
```

opening `routing/inbound.cfg` on its own resolves `send_to_carrier`,
offers it in completion, and reports nothing about it — even though
`inbound.cfg` neither defines it nor includes the file that does.

**The only thing the user must do is open the folder** (in VS Code,
`File → Open Folder…`), because the root is found by reading the
configs under the client's workspace folders. A client that opened a
single file has given the server nothing to read; the root could be
any directory above it, and guessing is worse than saying so. With no
folder, every config is a program of its own — the pre-0.20.0
behaviour.

The server keeps the workspace's include graph inverted — which
config names which — and climbs it to the top of the chain that
reaches the open document:

- **Diagnostics.** The analyzer runs over the root's closure, so the
  routes and modules the parent brings exist; only findings that land
  in the fragment are reported. `opensips -C` is run on the ROOT, and
  each error it reports is routed to the file it actually names — an
  error inside the fragment lands on the fragment's own line. If the
  program fails to compile for a reason that is not in this file, the
  fragment says so on line 1 and names where: a failed check must
  never render as clean.
- **Navigation and completion.** Go to definition, references,
  rename, call hierarchy, workspace symbols and route completion all
  span the root's closure, so a `route()` the parent defines resolves
  from inside the fragment and is offered while typing there.
- **`opensips/analysisRoot`.** A non-LSP request answering "what is
  this document a piece of": the root's URI, or `null` when the
  document is a program in its own right — or a file the workspace
  never includes. The VS Code client uses it to decide whether an
  unassociated `.cfg` on screen is part of an OpenSIPS configuration
  (see **Notes**).

A fragment reached from more than one root has no single true answer;
the lexicographically first parent is taken at each step, so the
context cannot flicker between edits. Include cycles terminate at the
last config not already visited.

`.` and `..` are folded when an include is resolved, so one file has
one name: a per-site layout that reaches shared routing as
`../common/routing.cfg` names the same file the editor opens as
`common/routing.cfg`. Without that they are two keys — the fragment
has no root and the closure visits it twice, so every route it
defines reads as defined more than once. The folding is textual, not
`canonicalize`: an include may name a file that does not exist yet,
and canonicalising would replace the path you see with the target of
any symlink on it. The one case where the two differ is a symlinked
directory, where the OS reads `sites/../common` relative to the
link's target and this does not.

The graph is rebuilt when a config is created, deleted or changed on
disk, when a document opens or closes, and when an edit adds or
removes an include directive — nothing else can move a file from one
root to another, so ordinary typing does not pay for it. **Typing the
`include_file` line re-checks the file it names**, if that file is
open: adding the include is the fix for the warnings that file was
showing, and leaving them up until it is next touched makes the fix
look like it did not work. A deeper change than one level corrects
itself on save, through the watched-files path.

The closure itself is bounded too — depth 8, 64 files — and a
configuration with one include per carrier passes 64 without trying.
A fragment past the bound is not in the closure built from its root,
so its OWN closure leads and the root's follows: analysing a file in
its parent's context must only ever ADD to what that file could
already see, never take its own includes away.

Answering a follow-up about a route defined in the root needs the
root's text, and while you are editing an include the root is not a
file the editor has opened. It is read from disk (same 1 MiB cap as
the include loader) rather than treated as empty — reading it as
empty is how call hierarchy came to answer "nobody calls this" for a
route the buffer on screen calls two lines up.

What counts as "a config this server can read" is decided in ONE
place, because two places is a disagreement: a file over 1 MiB is
skipped by both the graph and the closure, so a root can never be
found and then refuse to load, and bytes are decoded leniently, so a
latin-1 accent in a comment does not erase a configuration and every
fragment it includes along with it. A config the graph could not read
is named in the log, not dropped in silence.

The scan behind the graph is bounded at 500 configs and **says so in
the log** when it stops early. Past that bound a root is simply not
seen and its fragments stop being recognised — no colours, no
context — so a silent bound would be a disappearance with no
explanation anywhere.

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

Three things it will not touch:

- **Continuation lines of a multi-line string or block comment** —
  their leading whitespace is content, not layout.
- **`#!` lines** — OpenSIPS has no preprocessor (its `cfg.lex` has
  `COM_LINE #` and no preprocessor token), so these are comments and
  their column is the author's. The analyzer warns separately when one
  spells a Kamailio directive such as `#!ifdef`, because it guards
  nothing here.
- **Lines that continue the previous statement.** Brace depth is not
  the whole story about indentation here. A call whose arguments span
  lines, a condition broken across lines, and the body of a braceless
  `if` are all placed by the author to show what they belong to, and
  none of that shows up in the brace count. A line is only re-indented
  when the previous code line actually *ended* a statement — with `;`,
  `{` or `}`. Dedenting a braceless `if` body would be the worst of
  it: the parse would not change, but the body would read as though it
  runs unconditionally.

Indentation follows the editor: the `insertSpaces` and `tabSize` the
client sends with the request decide tabs versus spaces and the width.
Upstream `etc/opensips.cfg` is tab-indented, which is what a client
sending no preference gets.

The guarantee is tested three ways: the reformatted document must be
identical to the original once leading and trailing whitespace is
stripped from every line; formatting must be idempotent; and, against
a real binary, the positioned parse errors `opensips -C` reports must
be unchanged by formatting.

#### Pull diagnostics

`textDocument/diagnostic` answers for one document; `workspace/diagnostic`
sweeps the workspace without opening anything. A report carries a
result id, so asking again for something that has not moved comes back
`unchanged` instead of resending the same list.

**Only root configs are reported by the workspace sweep.** A config
that another config includes is a fragment, not a program: checked on
its own it would flag every route its parent defines as undefined and
every construct it continues as a syntax error. Roots are the files
nothing else includes, and their closures already cover the fragments,
so nothing is lost by leaving fragments out — and a great deal of
noise is avoided. The sweep is bounded at 500 configs and says so in
the log when it stops early; a truncated sweep that looks complete
would be worse than one that admits it.

**Pushing stops when the client pulls.** The two are separate channels
and a client that does both shows every problem twice, so the server
picks one based on what the client declared. Because the `-C` check is
asynchronous, a pulling client is answered from the previous checker
result and then sent `workspace/diagnostic/refresh` when the new one
lands — an invitation to ask again, which is how the protocol expects
an async server to behave.

#### Watched files

Two things the server derives answers from can change without ever
arriving as a document edit: a config included by an open file, and
the documentation tree itself. A git checkout, a rebuild, or another
tool editing an include all leave the server answering from a stale
read until the buffer happens to be touched.

The server registers for `workspace/didChangeWatchedFiles` on the
configs and on the tree, and reacts to each:

- **An include changed** — every open document whose include closure
  contains that file is re-checked and republished.
- **The tree changed** — the catalogue is re-harvested. The cache
  fingerprint is content-aware, so a changed file misses the cache by
  construction rather than by special casing.

A re-check driven by a watched file publishes even when the result is
clean. That is deliberate and differs from opening a file: if the
warning on screen is no longer true, saying nothing would leave it
there.

Registration is dynamic, so it only happens when the client declares
support, and the request is time-bounded — a client that declares
support and then never answers cannot stall startup. The tree usually
lives outside the workspace, so its watcher is a relative pattern
rooted at the tree.

#### Documentation before you configure anything

Two catalogues — the core language and every documented module — are
built in, harvested from OpenSIPS 4.0.1 and used when no source tree
is configured:

- **the core language** — parameters, functions and pseudo-variables
  like `log_level`, `socket`, `mpath`, which are not a module at all;
- **every documented module** — 186 of them, with their exported
  functions and parameters, so `loadmodule "` offers real names and a
  call like `is_method` completes and hovers.

Both are clearly labelled: hover any built-in entry and it says which
version the documentation came from and that setting `opensipsSrc`
gives you docs exact for your own build. A configured source tree
always wins, and **replaces** a built-in catalogue rather than merging
with it — blending two versions would be wrong in a way neither is on
its own.

Shipping the module half reverses an earlier decision, which said
there was no honest version to pin module docs to because what modules
exist depends on what you built. That objection was right about the
risk and wrong about the remedy: it applies equally to core parameters,
which move between releases too, and the answer in both cases is
provenance plus a total override rather than silence. Two things keep
it honest:

- **the loaded-module rule still holds** — a module's functions are
  offered only inside a config that `loadmodule`s it, so the built-ins
  never invite a call the config cannot make;
- **the checker has the last word** — `-C` loads the modules a config
  references, so a module you have not built is reported as a
  diagnostic on the `loadmodule` line itself.

What the built-ins cannot tell you is whether a module is installed on
*your* system: the name list is what 4.0.1 documents, not what you
compiled.

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
| `opensipsLsp.diagnostics.analyzer` | `analyzerDiagnostics` | — | `true` | Fast analyzer warnings between saves (undefined `route()` targets, duplicate definitions, undocumented modparams, inert `#!` directives). |
| `opensipsLsp.codeLens.references` | `codeLensReferences` | — | `true` | Reference-count code lenses on route definitions. |
| `opensipsLsp.inlayHints.parameterNames` | `inlayHintParameterNames` | — | `true` | Draw parameter names at documented call sites. |
| `opensipsLsp.checkTimeoutMs` | `checkTimeoutMs` | `OPENSIPS_LSP_CHECK_TIMEOUT_MS` | `10000` | Kill a `-C` run after this many ms. |
| `opensipsLsp.completion.snippets` | `snippetCompletions` | — | `true` | Function completions as tabstop snippets. |
| `opensipsLsp.cacheDir` | `cacheDir` | `OPENSIPS_LSP_CACHE_DIR` | platform cache dir | Documentation-catalog cache location. |
| `opensipsLsp.associateIncludedFiles` | — | — | `true` | Give a plain-text `.cfg` the workspace's configuration includes the OpenSIPS language (colours, completion, diagnostics). Files another extension already claims are left alone. |
| `opensipsLsp.trace.server` | — | — | `off` | LSP traffic tracing in the output channel. |
| — | — | `OPENSIPS_LSP_OUTPUT_CAP_BYTES` | `1048576` | Byte cap on captured `-C` output. |

## Notes

- The extension claims `opensips.cfg`, `opensips*.cfg` and
  `*.opensips.cfg` by name. The generic `.cfg` extension is
  deliberately left alone so unrelated tools' config files are not
  hijacked. A `.cfg` your configuration *includes* is picked up
  anyway, at runtime: VS Code hands it over as plain text, the
  extension asks the server whether anything includes it
  (`opensips/analysisRoot`) and sets the language when something
  does, so an include named `carrier-routes.cfg` gets the same
  colours and the same server as the root that pulls it in. A file
  another extension has already claimed is left to that extension.
  Turn this off with `opensipsLsp.associateIncludedFiles`; anything
  the server cannot reach through an include still needs a
  `files.associations` entry mapping it to `opensips-cfg`.
- Runtime toggles (`diagnostics.analyzer`, `completion.snippets`,
  `codeLens.references`, `inlayHints.parameterNames`,
  `diagnostics.maxProblems`, `checkTimeoutMs`)
  apply **live**: the client pushes them to the running server over
  `workspace/didChangeConfiguration` and open documents republish
  immediately — no restart. Settings that shape initialization
  (`serverPath`, `opensipsPath`, `opensipsSrc`, `cacheDir`, `enable`,
  `diagnostics.enable`) still restart the server automatically.
- Snippet completions and static snippets compose: static snippets
  scaffold blocks, completion snippets fill in calls.
- Include handling is capped for safety: depth 8, 64 files, 1 MiB
  per file (OpenSIPS itself caps includes at depth 20 —
  `CFG_MAX_INCLUDE_DEPTH` in `cfg_pp.h`). The LSP resolves
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
