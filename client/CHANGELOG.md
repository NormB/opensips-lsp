# Changelog

All notable changes to the OpenSIPS Routing Script extension.

## [0.18.1] — 2026-08-24

Documentation only; the server behaves exactly as 0.18.0 did.

- **Every editor is documented, not just VS Code.** The server speaks
  LSP 3.17 over stdio, so any LSP client can drive it, and
  `docs/EDITORS.md` now has worked examples for Neovim, coc.nvim,
  Helix, Emacs, Vim, Sublime Text, Kate and JetBrains (via LSP4IJ) —
  plus using `check` in CI or a git hook with no editor at all.
- **Zed has a step-by-step guide**, `docs/ZED.md`. Zed cannot be
  pointed at a language server from settings alone; it needs a small
  WebAssembly extension, and the guide's shell block builds one,
  including giving the tree-sitter grammar the repository of its own
  that Zed requires.
- **The examples were wrong in ways that mattered.** They told every
  non-VS-Code client to match `opensips.cfg` and nothing else, long
  after the association widened to `opensips*.cfg` and
  `*.opensips.cfg`; they told Helix to claim *every* `.cfg` file, which
  is the one thing this extension refuses to do; and they presented
  `opensipsSrc` as the way to get documentation, which stopped being
  true in 0.18.0. All corrected, and gated so they cannot drift again.

## [0.18.0] — 2026-08-24

- **Every module's functions and parameters now ship with the
  extension.** 0.17.0 gave you the core language before you configured
  anything; that left the half a real config is actually made of.
  `is_method` is a `sipmsgops` function, not core, so with no source
  tree `loadmodule "` offered nothing at all and a module call
  completed to the core entries and stopped. 186 modules, 352
  functions and 1300 parameters harvested from OpenSIPS 4.0.1 now ship
  alongside. Module functions still appear only inside a config that
  `loadmodule`s them — the built-ins never invite a call your config
  cannot make.
- **What the built-ins cannot know is what you compiled.** The module
  list is what 4.0.1 documents, not what exists on your system; the
  `-C` checker still reports a module it cannot load. Set `opensipsSrc`
  and your own tree replaces the built-in catalogues wholesale — never
  merged, because blending two versions is wrong in a way neither is
  alone.
- **Hover now reads the syntax before it guesses.** A global
  `log_level=2` and `modparam("opentelemetry", "log_level", 2)` are two
  different things that share a name, and your config already says
  which one is on screen: the first is the core parameter, the second
  is that module's. Previously, with a catalogue loaded, the global
  hovered as the module's parameter — and a module you had not even
  loaded could shadow the language. Both are fixed, and signature help
  resolved in the same wrong order.

## [0.17.1] — 2026-08-24

Documentation and test work; the server behaves exactly as 0.17.0 did.

- **The getting-started guide had the file-association rule wrong in
  both directions.** It said a config must be named `opensips.cfg` "or
  end in `.cfg`". Neither half was true: `opensips-proxy.cfg` is
  claimed, a plain `foo.cfg` is not, and refusing to claim every
  `.cfg` on your disk is deliberate. Corrected, and the gate that
  should have caught it now reads that page too — it only ever read
  the marketplace listing.
- **`README.md` still presented `opensipsSrc` as the only way to get
  documentation**, and the guide told you to go and set it when
  completion showed no docs. Wrong for every core entry since 0.17.0.
  Both now say the core language is documented out of the box, and a
  new gate holds every page a new user reads to that claim.
- Internally: the built-in catalogue is now proven over real LSP
  stdio rather than only at the library level, and test fixtures clean
  themselves up when a test fails instead of only when it passes.

## [0.17.0] — 2026-08-24

- **The core language completes out of the box.** Typing `log_` used
  to offer nothing until you pointed `opensipsSrc` at a source tree —
  19 control-flow keywords was the whole vocabulary. A catalogue
  harvested from OpenSIPS 4.0.1 (66 functions, 97 parameters, 119
  pseudo-variables) now ships with the extension and fills that in.
  Hover on any built-in entry says which version it came from; set
  `opensipsSrc` and your own tree wins, because only that is exact for
  the build you run. Module documentation is still tree-only — what
  modules exist depends on what you built.
- **`exit` no longer completes as `exit()`.** It is documented among
  the core functions, so with a tree configured it inserted a snippet
  for a statement written `exit;`. Statement keywords stay statements.
- **Hover no longer comes back empty for `exit` and friends**, where a
  bare keyword used to beat the documented entry purely by arriving
  first.

## [0.16.0] — 2026-08-24

- **`opensips*.cfg` is now recognised**, so `opensips-tls.cfg` and
  `opensips-local.cfg` get language support without configuring
  anything. `opensips.cfg` and `*.opensips.cfg` continue to work, and
  the generic `.cfg` extension is still deliberately not claimed.
- **The installer now tells you it opts you out of updates.**
  `install.sh` sideloads a VSIX, and an editor never offers updates
  for a sideloaded extension — it carries no marketplace metadata, so
  an install can sit many releases behind with nothing indicating it.
  The script, the PowerShell installer and the Getting Started guide
  now say so, and name the two ways out: re-run the script, or install
  from the Extensions view so updates arrive on their own.

## [0.15.2] — 2026-08-23

- **This listing now describes what the extension actually does.** It
  had fallen seven features behind — formatting, call hierarchy, inlay
  hints, include links, pull diagnostics, watched files and
  refactorings were all shipped and none of them were mentioned here.
- `docs/ADMIN.md` gains the missing `inlayHintParameterNames` option.
- The documentation gate now covers this page too, and the two checks
  that were supposed to catch the omissions — but derived their lists
  by hand, or scanned for text that rustfmt had wrapped — now derive
  from the source and ignore whitespace.

## [0.15.1] — 2026-08-23

- **Formatter: continuation lines keep their own indentation.** Run
  against a real 993-line production config the formatter wanted to
  change 25 lines, and every one was wrong — hanging argument indents,
  conditions broken across lines, and the body of a braceless `if`.
  Dedenting a braceless `if` body was the worst of it: the parse does
  not change, but the body reads as though it runs unconditionally.
  A line is now re-indented only when the previous code line actually
  ended a statement. The same config now wants one change, and that
  one is a genuine repair.
- The corpus sweep now checks the formatter too: every real config in
  the source tree must format idempotently and preserve its content.

## [0.15.0] — 2026-08-23

- **Extract into a route**: select whole lines inside a route body and
  the lightbulb lifts them into a `route[EXTRACTED]` of their own,
  leaving a `route(EXTRACTED);` call behind at the same indentation.
  It refuses a selection whose braces do not balance, one covering the
  block's own braces, and — deliberately — **one containing `return`**:
  `return` leaves the route it is written in, so moving it into a new
  route would make the statements after the extracted call start
  running when they did not before.
- **Remove duplicate `loadmodule` lines**: a second load of the same
  module is a hard parse error, not untidiness. Every occurrence after
  the first is removed, and nothing is reordered — load order decides
  module initialisation order.

## [0.14.0] — 2026-08-23

- **Pull diagnostics**: `textDocument/diagnostic` and
  `workspace/diagnostic`. The workspace sweep reports problems across
  the project without opening a file, and reports carry a result id so
  an unchanged document comes back `unchanged` rather than resending.
- **Only root configs appear in the workspace sweep.** A config that
  another config includes is a fragment, not a program — checked alone
  it would flag every route its parent defines as undefined. Roots are
  the files nothing else includes, and their closures already cover
  the fragments. The sweep stops at 500 configs and logs that it did.
- Pushing stops when the client pulls, so nothing is reported twice.

## [0.13.0] — 2026-08-23

- **Watched files**: an include or the OpenSIPS documentation tree
  changing on disk — a git checkout, a rebuild, another tool — now
  re-checks and re-harvests without the buffer being touched. Until
  now the server kept answering from a stale read until you happened
  to edit the file.
- A re-check driven by a watched file publishes even when the result
  is clean, which differs from opening a file on purpose: if the
  warning on screen is no longer true, saying nothing would leave it
  there.

## [0.12.0] — 2026-08-23

- **Inlay hints**: arguments at a documented call site are labelled
  with the parameter name the module's own documentation gives them,
  so `t_relay("udp", 1)` reads as
  `t_relay(flags: "udp", outbound_proxy: 1)` without the document
  changing. Only calls the catalogue knows are hinted — which is what
  keeps `if`, `while` and `route` out — and a call with more arguments
  than the signature documents is hinted only as far as the signature
  goes. Bracket markers and defaults are stripped to the name.
  The editor asks for the visible range and only that is computed.
  `opensipsLsp.inlayHints.parameterNames` turns them off, live.

## [0.11.0] — 2026-08-23

- **Call hierarchy**: `textDocument/prepareCallHierarchy`,
  `callHierarchy/incomingCalls` and `callHierarchy/outgoingCalls`.
  Shift+Alt+H on a route name opens the route call graph — who calls
  `route[X]`, and what `route[X]` calls — across the include closure,
  so a caller in an included file shows up with that file's URI.
  Several calls from one block collapse into a single entry carrying
  each call site's range. A route called but defined nowhere is still
  listed as an outgoing edge, marked `undefined`, rather than dropped.
  The graph is the main route table: a `failure_route` shows up as a
  caller when it calls `route(X)`, but asking for *its* callers
  declines, because it is armed by a module-function string the server
  does not track and "no callers" would be a confident wrong answer.

## [0.10.0] — 2026-08-23

- **Formatting**: `textDocument/formatting` and
  `textDocument/rangeFormatting`. Shift+Alt+F and format-on-save now
  re-indent an `opensips.cfg` by brace depth and strip trailing
  whitespace, following the editor's tab settings.
  The formatter is deliberately line-preserving — it rewrites the
  leading and trailing whitespace of a line and nothing else, never
  joins, splits or reorders lines, never touches a string or comment
  body, and never emits an edit for a line that is already correct, so
  folding and cursor position survive. Braces inside strings and
  comments do not move the indent depth; `#!` directives keep their
  column. Proven against a real 4.0.1 binary: the positioned parse
  errors `opensips -C` reports are unchanged by formatting.
- **A skipped test is now a failed test.** Ten tests used to opt out
  silently when no OpenSIPS tree or binary was present, so CI reported
  green while the proofs behind this extension's version claims never
  ran. They are hard failures now, `scripts/proof-env.sh` provisions
  what they need, and CI runs that same script — a green build means
  the proofs really ran, against a real 4.0.1 tree and binary.

## [0.9.1] — 2026-08-23

- **Documentation drift gate**: the README and the features page are
  now checked against the server itself — every capability advertised
  in `initialize`, every initialization option and environment
  variable the server reads, and every VS Code setting the client
  contributes must appear in the docs, or the build fails.
- Fixes the drift that gate found: the README never mentioned include
  links (Ctrl+Click on `include_file`/`import_file`), `prepareRename`,
  `semanticTokens/range`, or live reconfiguration — all shipped
  earlier and all invisible to anyone reading the front page.
- The settings table in the features page was split in two by a
  paragraph wedged between its rows, so five settings rendered outside
  the table on GitHub and on the extension listing. The paragraph
  moved to the notes and the table is whole; the gate now fails if a
  stray line ever splits it again.
- Corrects a stale note claiming every setting change restarts the
  server — runtime toggles have applied live since 0.8.0.

## [0.9.0] — 2026-08-23

- **Language-server stack moved to `tower-lsp-server`**: `tower-lsp`
  has had no release since 0.20.0 (August 2023); the maintained
  community fork replaces it. No behaviour changes — the protocol
  suite (raw JSON-RPC over stdio) passes unchanged, and
  `workspace/symbol` still answers with the same array on the wire.
  Dropping the `url` crate takes its whole ICU/idna tree with it: the
  server now builds from 67 dependencies instead of 96.
- **Version-proven against OpenSIPS 4.0.1**: the proof suite now runs
  against tag-built trees and binaries for both 4.0.1 and 3.6.8,
  rather than against a moving master branch.
- A verbatim 4.0.1 `-C` capture joins the 3.6.8 one in the
  diagnostics tests. 4.x wraps its positioned errors in a traceback
  header, a caret marker, and an echo of the offending config lines;
  all of it is proven to stay noise, so one bad `modparam` still
  raises exactly one diagnostic.

## [0.8.0] — 2026-08-20

- **Route-family namespaces**: `route(x)` resolves only against the
  main table (`route[x]`) — navigation, references, highlight, and
  rename never cross into a same-named `failure_route[x]` or other
  block kinds, matching OpenSIPS semantics.
- **Checker cwd parity**: the server runs `opensips -C` from the
  config's own directory, exactly like the CLI — relative includes
  and module paths resolve identically everywhere.
- **Latest-wins checks**: a newer save kills a superseded `-C` run
  (queued or executing); slow parses no longer block fresh results.
- **prepareRename**: F2 is blocked off-symbol and pre-selects the
  route name otherwise.
- **Include links**: `include_file`/`import_file` paths are
  Ctrl+Click document links.
- **Semantic tokens range** requests for viewport-sized responses.
- **Live settings**: runtime toggles (analyzer, snippets, code lens,
  maxDiagnostics, checkTimeoutMs) apply without a server restart.
- **Content-aware doc cache**: editing module docs in place now
  invalidates the harvest cache (was: directory mtimes only).
- **Harvest progress + misconfiguration warning**: the doc harvest
  shows editor progress; a configured tree yielding no documentation
  raises a visible warning.
- **Narrowed file association**: only `opensips.cfg` and
  `*.opensips.cfg` are claimed — the generic `.cfg` extension is left
  alone (use `files.associations` for custom names).
- Internal: grammar↔scanner drift gate; per-version scanner
  memoization for hot requests.

## [0.7.1] — 2026-08-20

- **Semantic-token fix**: comments are now excluded byte-by-byte
  through the same classifier the analyzer uses. A `#` inside a
  string no longer hides the pseudo-variables after it, and pvars
  inside `/* ... */` block comments (same-line or multi-line) no
  longer receive tokens.
- Internal: drift gates for the release workflow; ground-truth
  re-audit against current OpenSIPS master (no grammar drift, all
  gates green).

## [0.7.0] — 2026-08-20

- **Workspace symbols** (Ctrl+T): route definitions searchable across
  every open file and its includes.
- **Code lenses**: reference counts above named `route` blocks,
  counted across the include closure
  (`opensipsLsp.codeLens.references`).
- **Quick fixes**: load the module that exports an
  `unknown command <f>` function; create a stub for an undefined
  `route(x)` target.
- **Catalog-pinned validation**: `modparam` parameters not documented
  by YOUR configured source tree warn as you type — version-exact by
  construction.
- **Semantic highlighting**: route names and pseudo-variables
  (including inside double-quoted strings, where OpenSIPS
  interpolates them).
- **CLI check mode**: `opensips-lsp check [--strict]
  [--bin <opensips>] <file>...` — the same analyzer plus the real
  parser, for CI pipelines and git hooks.

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
