# Changelog

All notable changes to the OpenSIPS Routing Script extension.

## [0.24.1] — 2026-09-02

**A dependency release: the language client the extension talks to,
and the type stubs it compiles against.**

- **`vscode-languageclient` 10.1.0 to 10.1.1**, which carries LSP
  3.18.3. Two fixes matter to how this extension behaves: diagnostic
  pull state now survives a document being closed and reopened
  quickly — the shape of a fragment being clicked through and come
  back to — and the client's stream handlers are configurable.
  Nothing in this extension's own code changed to meet it.
- **`@types/node` 26.2.0 to 26.4.0**, a compile-time stub with no
  runtime half.
- **The lockfile agrees with the manifest again.** Its own `version`
  field had drifted to 0.19.2 while the manifest read 0.24.0.
  Regenerating the tree resynced it; `npm ci` compares the two, so
  the pairing had been quietly untrue for several releases.

## [0.24.0] — 2026-08-26

**Hovering a setting now tells you what to write, and the tail of a
`socket` line stops being invisible.**

- **A parameter's hover carries its default and its worked example.**
  `db_default_url` used to say "the default DB URL used by modules
  when no per-module URL is configured" and stop — while the URL
  FORMAT, the one thing anyone hovers it for, sat in the example
  block underneath. It was not one parameter: 68 of 75 stated
  defaults and 96 of 99 examples were being discarded. All 97
  parameters now carry their example and 70 carry a default.
- **The twelve modifiers a `socket` line takes now hover and
  complete.** `use_workers n`, `reuse_port`, `anycast`, `as
  ip[:port]`, `tag ID`, `frag`, `tos n`, `accept_subdomain` and the
  three proxy-protocol forms. `socket` hovered and every word after
  it — most of what a real socket line is made of — answered nothing.
  The set is read from the grammar's own `socket_def_param`
  production, so it is what your release accepts; the manual supplies
  the descriptions. `listen =` is the same statement in 3.x and works
  the same way.
- **Spellings the manual skips now hover as the setting they are.**
  `workdir` is `wdir`, `tcpthreshold` is `tcp_threshold`, and three
  more. Writing the spelling upstream chose not to document used to
  answer nothing at all.
- **Names no manual page describes are offered anyway, and say so.**
  `memdump`, `memlog` and the calls `xdbg()` and `error()` are
  accepted by the grammar and documented nowhere. Offering nothing
  for them claims they do not exist.

### Fixed

- **An alias of a call is offered as a call.** No spelling in this
  release is affected — the sibling server's `seturi`/`rewriteuri`
  is — but the code path filed every alias under parameters, which
  offers it where a parameter belongs and hides it where the call
  does.
- A markdown list survives as a list. `socket` documents its twelve
  modifiers one bullet each, and they were being joined into a single
  unbroken paragraph.
- A `#` at column zero inside an example is a configuration comment,
  not a heading — there are 42 across the shipped manual, and a
  section ending at one lost the rest of its example.

## [0.23.0] — 2026-08-26

**The things a configuration is actually written with now hover and
complete: `$var`, `$avp`, and the log level `xlog` wants.**

- **`$var` and `$avp` are in the catalogue.** The two variables every
  configuration uses were the two the server did not have. Upstream
  documents its 119 reference variables as `### <description> -
  $name` and documents these two as prose sections opening with a
  `**Naming**:` line — so the harvester, reading only the first form,
  read past both. A section with a naming line is a variable kind
  now, unless it has entries of its own beneath it: `## Reference
  Variables` names `$name`, which is the placeholder those hundred
  entries share and not something you can write.
- **Typing `xlog(` offers the log levels, quoted.** `L_ALERT`,
  `L_CRIT`, `L_ERR`, `L_WARN`, `L_NOTICE`, `L_INFO`, `L_DBG` — read
  from the `switch` in your own tree's `route.c`, so it is the set
  your release accepts rather than a list frozen in this extension.
  The quotes come with it because the grammar wants a string there
  and `xlog(L_INFO, ...)` is a syntax error; type `xlog("` first and
  they arrive unquoted instead. A `$` still offers pseudo-variables,
  which that argument also accepts.
- **Ctrl+Alt+H turns the hovers and completion off** (Cmd+Alt+H on
  macOS), and the same keys turn them back on. For reading someone
  else's configuration, or showing one to a room, when the popups are
  in the way. It applies at once — no restart — the status bar reads
  `OpenSIPS hints off` so a quiet editor is not mistaken for a broken
  one, and diagnostics are left alone. `opensipsLsp.assistance` is
  the setting behind it.

### Fixed

- **A repeated heading in a manual page no longer becomes a second,
  unreachable entry.** Every lookup takes the first, so the other sat
  in completion and could never be hovered.
- **A heading with nothing under it is no longer offered.** It hovered
  a header over an empty body, which reads as the server being broken
  rather than as the page being thin.
- **A capitalised heading still matches its keyword.** `## If` became
  a name no hover could ever match, and the keyword vanished with no
  error anywhere.

## [0.22.0] — 2026-08-25

**Hover stops repeating the release, and you decide whether it says it
at all.**

- **The release is no longer under every hover.** OpenSIPS 0.x showed it on
  the status bar the whole time a config is open, and every warning
  that turns on the release names it — so repeating it under each
  hover and completion item was the same fact a third time. It is off
  by default now.
- **`versionInHints` turns it back on.** The note is not useless:
  reading a hover in isolation, or pasting one into a ticket, is
  exactly when you want the provenance travelling with the text. What
  was wrong was having it in front of everyone who does not.
- **The note now says which catalogue it describes, and names the
  release you are actually using.** It used to be baked into the
  catalogue as it loaded, so it named whatever the vendored file
  carried regardless of your choice. Module documentation follows
  `opensipsVersion`; core documentation is a single vendored artefact at
  4.0.1 and does not move with that setting, so it says so rather
  than claiming a release those docs did not come from.
- **Choosing a release is documented.** `GETTING_STARTED.md` covers
  the dropdown, `settings.json`, the environment variable `check`
  reads, and pointing at your own tree for a build that is not
  shipped.

## [0.21.0] — 2026-08-25

**The `modparam` check now knows what a module actually exports, and
the server follows the includes OpenSIPS actually opens.** Both had
been read out of the documentation, and the documentation is not the
authority for either.

- **Parameters come from the module's own code.** A module declares
  what `modparam` accepts in a C table, and that table is what
  OpenSIPS checks against when it starts. The catalogue is now built
  from it: 189 parameters the documentation never mentions are
  available in completion and hover for the first time — among them
  `rtpengine`'s `extra_failover_error`, `tcp_mgm`'s
  `connect_timeout_col` and all fifty of `b2b_sca`'s `appN_*` column
  names — and 25 entries the documentation invented are gone,
  including five `b2b_sca` heading templates and `tls_mgm`'s
  `cipher_list_col`, none of which OpenSIPS has ever accepted.
- **The release in use is on the status bar.** While a OpenSIPS config
  is in front of you, the status bar names the catalogue your
  `modparam` lines are being checked against — `OpenSIPS 4.0.1`, or
  the configured source tree when you have pointed at one. Previously
  the only way to find out was to write something wrong and read the
  warning. The server tells the editor directly, so it is right even
  when the release was chosen for you by a workspace setting.
- **The release is a dropdown, not a text box.** `opensipsVersion`
  lists exactly the releases the shipped catalogue can answer for, so
  it cannot be mistyped into a value the server has to reject.

- **Choose the release you run.** The built-in catalogue now covers
  OpenSIPS 3.5.9, 3.6.8 and 4.0.1, and `opensipsLsp.opensipsVersion`
  picks which one your configuration is judged against. A warning
  names that release, and when the parameter exists in another one it
  says which: *"'bin_async_local_connect_timeout' is not exported by
  module 'proto_bin' in OpenSIPS 4.0.1 — it exists in 3.5.9, 3.6.8"*.
  That is the difference between a warning you have to investigate and
  one that explains itself.
- **An include inside a block comment is followed, because OpenSIPS
  follows it.** Includes are flattened line by line before the config
  is parsed, so commenting a section out with `/* */` does not stop
  its `include_file` lines from loading. The server used to miss those
  files entirely: the routes they define were invisible and every call
  into them was reported undefined. It also stopped following a
  directive written after a statement on the same line, which OpenSIPS
  ignores.
- **`!!include_file` is no longer shown as valid.** OpenSIPS rejects
  it outright — it is a syntax error, not a prefixed include — and the
  grammar had been colouring it as though it worked. `#!include_file`
  is different again: a comment that does nothing, still reported as
  having no effect.
- **`opensips-lsp check` runs the same `modparam` check the editor
  does.** A CI job or git hook previously ran only the analyzer, so a
  configuration naming a parameter no module exports passed there
  while the editor warned about it. Expect `check` to report warnings
  it did not report before; they were always true.
- **Two warnings arrived with a gap in the middle of the sentence**,
  from a line continuation that kept its indentation. Both read as
  sentences now.

## [0.20.1] — 2026-08-25

**A split configuration whose fragments are named `.inc` now works,
and its `modparam` lines stop being contradicted.** 0.20.0 taught the
server to analyse an included file in the context of its root, but
both halves of the feature still guessed at filenames, and the module
catalogue behind the `modparam` check was missing a third of what the
documentation says.

- **A fragment gets the language whatever it is called.** The
  extension asked the server about a plain-text file only when it was
  named `*.cfg` — the same filename guess the request exists to avoid.
  A tree of `include/*.inc` fragments got no colours, no completion
  and no diagnostics. The suffix test is gone: any plain-text file is
  asked about, and one nothing includes is still left alone.
- **The workspace sweep looks for `.inc` and `.m4` too.** It collected
  `*.cfg` only, so in a tree whose root or fragments are named
  anything else no fragment ever resolved a root and every one of them
  was analysed alone. It is still a search for configurations, not a
  read of every file in the folder.
- **52 wrong `modparam` warnings, on one real configuration.** The
  documentation harvester read any line starting with `#` as a
  heading, so the `# single rtproxy` comment inside rtpengine's first
  example closed the Exported Parameters section and the thirteen
  parameters below it were never harvested. Presence, dispatcher,
  registrar, tracer, prometheus and eleven other modules lost
  parameters the same way. A `##### ` sub-heading inside one
  parameter's prose ended the section too, and `db_url(str)` — the
  type written with no space — was harvested with the type inside the
  name, where no `modparam` could ever match it.
- **A heading that lists several parameters documents each of them.**
  `osp` writes `private_key, local_certificate, ca_certificates` under
  one heading and `tls_mgm` writes `server_domain, client_domain`;
  sixteen parameters were unreachable between them.
- **Seven more modules complete after `loadmodule "`.** `xml`,
  `event_datagram`, `db_perlvdb`, `presence_mwi`, `presence_xcapdiff`,
  `tls_openssl` and `auth_web3` export nothing, and a module that
  exports nothing was being dropped from the catalogue entirely.
- **A module whose documentation could not be read no longer accuses
  your configuration.** `auth_web3` writes its README in a shape the
  harvester does not read; an empty parameter list is not evidence
  that a parameter does not exist.

The catalogue is now checked against the READMEs' own `modparam`
examples — 1,749 of them across the pinned 4.0.1 tree — so a harvest
that silently loses parameters fails CI rather than reaching you as a
warning about a parameter that exists.

## [0.20.0] — 2026-08-25

**Opening an included file on its own now works.** A configuration
split across `include_file`/`import_file` is the normal shape of a
real deployment, and until now every file in one except the root was
second-class: no colours, and a screenful of warnings about routes
its parent defines.

- **It gets the language.** The extension still refuses to claim every
  `.cfg` on your disk — that hijacks unrelated tools' config files —
  but a fragment named `carrier-routes.cfg` is no longer left as plain
  text. It asks the server whether anything includes the file and sets
  the language when something does, leaving files another extension
  already claims alone. Turn it off with `opensipsLsp.associateIncludedFiles`.
- **It is analysed as part of its root.** Routes, modules and defines
  the parent brings are in scope, so they stop reading as undefined —
  on the configuration this was developed against, splitting it across
  sixty files produced seventy-five false warnings before and none
  after. `opensips -C` runs on the root, with each error routed back to
  the file it actually names.
- **Navigation spans the whole configuration.** Go to definition,
  references, rename, call hierarchy, workspace symbols, reference
  counts and route completion all reach across the include boundary
  from inside a fragment.
- **`opensips-lsp check` agrees with the editor.** The command a git hook and
  a CI job run now finds the root the same way, so a correct split
  configuration no longer fails the build under `--strict`.
- **New request `opensips/analysisRoot`** for editors other than VS Code:
  what is this document a piece of? The root's URI, or `null` when it
  is a program in its own right.

**One thing to do:** open the folder, not the single file. The root is
found by reading the configs under your workspace folders, so with a
single file open there is nothing to read. Folders added after startup
are picked up.

- **A file behind a `#!ifdef` IS part of your configuration**,
  because OpenSIPS reads it either way — verified against the 4.0.1
  binary. Kamailio compiles such includes out and its extension skips
  them; the two engines genuinely differ here.

Limits are announced rather than silent: a workspace past 500 configs,
and any config too large or unreadable to include in the graph, say so
in the output channel.

Also fixed, and unrelated to the above: a failed `opensips -C` run that
reported no position used to render as `check failed in , line 1: …`,
naming an empty file and a line the parser never gave.

## [0.19.2] — 2026-08-24

Documentation only; the server and the extension behave exactly as
0.19.1 did.

- **What this extension does with your configuration is now written
  down.** Someone evaluating it asked whether anything parsed from
  their `opensips.cfg` is sent to a cloud service or a language model
  — a fair question about a file that carries customer names,
  credentials and dial plans, and one no page here answered. The
  answer was already no: no HTTP client is linked into the server, the
  transport is stdin/stdout, the only program it runs is your own
  `opensips` binary, and hover and completion text is parsed from
  OpenSIPS's documentation on disk rather than generated. This
  listing, the README and `SECURITY.md` now say so — with the two
  caveats that matter, that `trace.server` echoes cfg text into the
  editor's output channel and that your editor's own telemetry is not
  this server's to control.
- **The no-network property is now a reportable security claim.**
  `SECURITY.md` puts any path by which configuration content, file
  paths or environment values reach the network in scope, which makes
  the guarantee falsifiable instead of promotional.

## [0.19.1] — 2026-08-24

Nothing changes for VS Code users; this fixes the tree-sitter grammar
for everyone else.

- **The grammar could not be built by any editor it was offered to.**
  `src/` was gitignored, so the generated `src/parser.c` was in no
  commit — and that is the file every consumer builds from, none of
  them running the CLI: nvim-treesitter fetches
  `files = { "src/parser.c" }`, Helix and Zed compile that path
  directly. The README has pointed Neovim, Helix and Zed users at this
  grammar all along and not one of them could have used it. The
  generated output is now committed, which is the conventional layout
  for a tree-sitter grammar, and a gate regenerates and compares so it
  cannot go stale.
- **The Zed guide drops a workaround it never needed.** It was cloning
  the repository and giving the grammar directory a git root of its
  own, on the belief that Zed required one. Zed takes a `path` for a
  grammar in a subdirectory — as do Helix (`subpath`) and
  nvim-treesitter (`location`) — so the guide now points at the public
  repository directly.

## [0.19.0] — 2026-08-24

- **`#!` directives are now flagged as having no effect.** OpenSIPS has
  no preprocessor — its lexer starts a line comment at `#` and defines
  no preprocessor token at all — so `#!ifdef USE_TCP` guards nothing
  and `#!define X 5060` binds nothing. A config carried over from
  Kamailio still parses while every conditional in it has quietly
  stopped meaning anything, which is worth saying out loud. The
  warning fires only on a comment that starts at that position and
  only for keywords Kamailio's own lexer defines, so shebangs,
  ordinary comments, and a directive parked inside a block comment are
  left alone. Toggle with `opensipsLsp.diagnostics.analyzer`.
- **The docs said the opposite.** `docs/FEATURES.md` and a comment in
  the formatter both described `#!` lines as preprocessor directives
  processed ahead of the parser. That was mirrored from the Kamailio
  server, where it is true. Corrected, with the reason.
- Internally: the analysis cache is now instrumented and gated, so a
  change that recomputed on every keystroke would fail a test rather
  than show up as a hot editor; CI gained an advisory gate
  (`cargo audit`) and an MSRV job that builds with the exact toolchain
  `rust-version` claims; and the client is installed from a lockfile
  rather than resolved afresh on every build.

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
