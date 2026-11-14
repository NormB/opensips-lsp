# Changelog

All notable changes to the OpenSIPS Routing Script extension.

## [0.28.3] — 2026-11-14

### Changed

- **The release builds on a supported macOS image.**
  - Effect: the macOS servers are built on `macos-26` and still run on
    macOS 11.0 (arm64) and 10.12 (x86_64) or later.
  - Effect: no behavior change; this release also carries the outline
    changelog of 0.28.2.
  - Limit: 0.28.2 is a tag without a release; its build named the retired
    `macos-14` image and published nothing.

## [0.28.2] — 2026-11-14

### Changed

- **The changelog is rewritten in an outline format.**
  - Effect: every entry states what changed, what to do and where the
    detail is.
  - Effect: documentation only; the extension and the server behave
    exactly as 0.28.1 did.

## [0.28.1] — 2026-11-03

### Added

- **Each release carries `SHA256SUMS.txt`, a Sigstore signature and SLSA
  build provenance.**
  - Effect: a download can be verified with cosign or
    `gh attestation verify`.
  - Docs: [Verifying a release](https://github.com/NormB/opensips-lsp/blob/main/SECURITY.md#verifying-a-release).

### Security

- **GHSA-g3hv-75wg-m249 (minimatch pattern amplification) does not affect
  the extension.**
  - Effect: no code in the extension or the server changed.
  - Limit: minimatch 10.2.6 stays until vscode-languageclient supports the
    fixed minimatch 11.
  - Docs: [Advisories that do not affect this project](https://github.com/NormB/opensips-lsp/blob/main/SECURITY.md#advisories-that-do-not-affect-this-project).

## [0.28.0] — 2026-10-27

### Added

- **Renaming or moving a file rewrites every `include_file`/`import_file`
  path that names it.**
  - Effect: includes across the workspace are rewritten before the rename;
    relative paths stay relative, absolute paths stay absolute.
  - Docs: [Renaming an included file](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#renaming-an-included-file).
- **An include closure that stops at 500 files is reported on the first
  line.**
  - Effect: the editor and `opensips-lsp check` warn, because a route in
    an unread file is reported as undefined.
  - Docs: [An included file opened on its own](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#an-included-file-opened-on-its-own).
- **`has_body` completes, hovers and shows signature help.**
  - Effect: it is an alias of `has_body_part`; sipmsgops exports both
    names with the same handler.

### Fixed

- **A failed `opensips -C` check always reports a message.**
  - Was: when the last error line had no text after its prefix, the
    diagnostic was empty and the editor showed nothing.
  - Effect: the nearest earlier line with a reason is used, or a fixed
    fallback text.
- **A documentation heading or DocBook id that names nothing adds no
  parameter.**
  - Was: `#### (integer)` or an id of `param_` made an empty-named
    parameter, offered as a completion and checked against every
    `modparam`.

## [0.27.0] — 2026-10-26

### Added

- **Inside `include_file "` or `import_file "`, completion lists
  directories and configuration files.**
  - Effect: `.cfg`, `.inc` and `.m4` files and directories complete
    relative to the including file; typing `/` opens the next directory.
  - Was: keywords and function names were offered there.

### Changed

- **The include closure follows 19 levels and reads up to 500 files.**
  - Was: it stopped at 64 files and 8 levels, so routes in later files
    were reported as "not defined here or in included files".
  - Effect: `route()` calls and go to definition reach routes in later
    include files; `opensips-lsp check` uses the same closure.
  - Limit: one level deeper fails in OpenSIPS 4.0.2 with "max nested cfg
    files reached".
  - Docs: [An included file opened on its own](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#an-included-file-opened-on-its-own).
- **vscode-languageclient moves from 10.1.2 to 11.0.0 and tokio to
  1.54.1.**
  - Effect: the extension and the server are built with these and the
    other semver-compatible crate updates.

### Fixed

- **Five functions documented under shared headings now complete, hover
  and show signature help.**
  - Effect: `uac_replace_to`, `uac_restore_to`, `rtpproxy_stream2uas`,
    `rtpproxy_stop_stream2uas` and `t_write_unix` are in the 3.6.9 and
    4.0.2 catalogs.
- **The 3.5.9 catalog gains 27 functions and loses 7 names no release
  exports.**
  - Was: the `h350`, `ldap`, `lua`, `rate_cacher` and `rtpproxy` stream
    functions, `change_reply_status` and `stir_shaken_disengagement` were
    missing.
  - Effect: `get_dialogs_by_val` and `call_transfer` replace
    `get_dialog_by_val` and `call_blind_transfer`, names taken from
    section ids.
- **`Exported Events` and `Exported Pseudo-Variables` are no longer listed
  as functions.**
  - Was: the `janus` and `rr` modules listed them as functions.
- **A top-level parameter after a blank line appears once in the
  outline.**
  - Was: it was listed twice, the second time with a range the LSP
    specification does not allow.

## [0.26.3] — 2026-10-22

### Changed

- **No behavior change: the language server is rebuilt against tokio
  1.53.3.**
  - Effect: tokio, the server's async runtime, moves from 1.53.2 to
    1.53.3; server and extension code are unchanged from 0.26.2.

## [0.26.2] — 2026-10-19

### Changed

- **The built-in catalog moves to OpenSIPS 4.0.2 and 3.6.9.**
  - Effect: the `opensipsVersion` choices are `3.5.9`, `3.6.9` and
    `4.0.2`; the core parameter `REACTOR_DBG_FD_CHECK` completes and
    hovers.
  - Do: set `3.6.9` to keep checking against the 3.6 line.
  - Limit: a setting of `3.6.8` or `4.0.1` is reported as unrecognized and
    `4.0.2` is used.
  - Docs: [Checking against a specific OpenSIPS release](https://github.com/NormB/opensips-lsp/blob/main/docs/GETTING_STARTED.md#checking-against-a-specific-opensips-release).
- **The documented catalog counts match what the server reports.**
  - Effect: the pages state 193 modules and 68 core functions.
  - Limit: the real-binary proof runs against OpenSIPS 4.0.2; 3.6.9 and
    3.5.9 are checked against their source trees.

### Fixed

- **A `#`, quote or brace inside a `$(...)` variable no longer breaks the
  outline.**
  - Was: `$ru = $(ru{re.subst,/a#//g});` made its route run to the end of
    the file in the outline.
  - Effect: inside `$(...)` or `$name(...)`, `#` starts no comment, a
    quote no string and a brace no block.

## [0.26.1] — 2026-10-15

### Changed

- **No behavior change: two weekly dependency updates.**
  - Effect: `serde_json` moves from 1.0.151 to 1.0.154 and `@types/node`
    from 26.6.4 to 26.6.5; no source file changed.

## [0.26.0] — 2026-10-11

### Added

- **The outline nests `if`, `else`, `switch` and `while` blocks under
  their route.**
  - Effect: blocks are named by their condition; every `loadmodule`,
    `modparam` and top-level parameter is listed.
- **Folding covers blocks inside a route, multi-line block comments and
  runs of `#` comment lines.**
  - Effect: a `} else {` line stays visible when the block above it is
    folded.
  - Docs: [Folding](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#folding).
- **Expand selection grows from a word to each enclosing block, the route
  and the file.**
  - Effect: the server answers `textDocument/selectionRange`.
  - Docs: [Selection range](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#selection-range).

### Changed

- **The formatter indents what a `case` or `default` label guards one
  level under it.**
  - Was: the line after a label was left where it was.
  - Docs: [Formatting](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#formatting).

### Fixed

- **Opening files no longer holds up other requests.**
  - Was: five files opened together left a request waiting about 6 s
    behind their checks.
  - Effect: a check that runs `opensips -C` runs on its own task.
- **Workspace diagnostics agree with the include tracking.**
  - Was: a fragment included as `sub/../frag.inc` was reported as a
    program of its own, and a config with a latin-1 byte was skipped.
  - Effect: `opensips-lsp check` reads a config with a latin-1 byte, where
    it stopped with "cannot read".
- **`.inc` and `.m4` fragments edited on disk are noticed.**
  - Was: the file watcher covered only `*.cfg`.
- **Single quotes work everywhere double quotes do.**
  - Effect: `loadmodule 'tm.so'` loads the module; completion after
    `loadmodule '`, `modparam('` and `route('` offers what it offers
    after `"`.
  - Effect: signature help counts arguments across `'a,b'`, and a
    backslash inside `'...'` no longer escapes the closing quote.
- **Completion offers nothing inside a comment.**
  - Effect: typing in a comment opens no completion list.
- **Call hierarchy works on a line with non-ASCII text before the route
  name.**
  - Was: the cursor column was read as a byte offset.

## [0.25.0] — 2026-10-11

### Added

- **The resume route of `async(f(...), x)` and `launch(f(...), x)` is a
  reference to `route[x]`.**
  - Effect: references, rename, the code lens, call hierarchy and the
    "route is not defined" warning see it.
- **The tree-sitter grammar parses every config in the OpenSIPS 4.0.1 tree
  that `opensips -C` accepts.**
  - Effect: nested block comments, multi-line and adjacent strings,
    bitwise operators, `not`, every assignment operator and socket
    parameters parse.
  - Effect: `return -1;`, IPv4 addresses and networks and `include_file`
    inside a route parse; `!$a == 1` parses as `!($a == 1)`.
  - Do: editors that build the grammar compile `src/scanner.c` along with
    `src/parser.c`.
- **The grammar ships `queries/highlights.scm` and `queries/folds.scm`.**
  - Effect: the Zed setup script downloads the highlight query into the
    extension it builds.
  - Docs: [Zed setup](https://github.com/NormB/opensips-lsp/blob/main/docs/ZED.md).

### Changed

- **Rename refuses a name that is already defined in the same namespace.**
  - Effect: the refusal names the file that defines it; the result would
    define the route twice, which `opensips -C` rejects.

### Fixed

- **Requests no longer slow down with the square of the file size.**
  - Was: one scanner took 73 s on a 6000-route config; it now takes under
    half a second.
  - Effect: on a 3000-route document the code lens no longer times out.
- **Renaming a `failure_route`, `onreply_route` or `branch_route` rewrites
  the strings that arm it.**
  - Was: only the definition was edited, leaving `t_on_failure("x")`,
    `t_on_reply("x")` or `t_on_branch("x")` naming a route that no longer
    existed.
  - Effect: references, highlights and go to definition see those strings
    too.
- **Formatting a CRLF file no longer doubles its carriage returns.**
  - Was: every reformatted line ended in `\r\r\n`.
- **`opensips-lsp check --bin ... fn.cfg` works with a relative path.**
  - Was: a bare file name failed with "cannot run", and `sub/fn.cfg` was
    handed to the checker from inside `sub/`.
- **Nested block comments are comments.**
  - Was: `/* a /* b */ route(x); */` reported `x` as undefined, and
    `check --strict` failed on it.
  - Effect: OpenSIPS nests `/* */`, and the server now reads it the same
    way.

## [0.24.7] — 2026-10-07

### Changed

- **No behavior change: two weekly dependency updates.**
  - Effect: `tokio` moves from 1.53.1 to 1.53.2 and `@types/node` from
    26.6.3 to 26.6.4; no source file changed.

## [0.24.6] — 2026-10-01

### Changed

- **No behavior change: one weekly dependency update.**
  - Effect: `vscode-languageclient` moves from 10.1.1 to 10.1.2 and
    `@types/node` from 26.6.2 to 26.6.3; no source file changed.

### Security

- **`brace-expansion` moves from 5.0.9 to 5.0.12, which fixes three
  advisories.**
  - Was: the extension bundles it through `minimatch`, and 5.0.9 is
    affected by three advisories published 2026-09-29.
  - Effect: GHSA-6j4f-fj2g-mc7p (high, fixed in 5.0.10) and
    GHSA-qhr7-859c-m2p7 (high, fixed in 5.0.11) no longer apply.
  - Effect: GHSA-q2hr-2g5m-vwhr (moderate, fixed in 5.0.12) no longer
    applies.

## [0.24.5] — 2026-09-26

### Changed

- **No behavior change: one weekly dependency update.**
  - Effect: `@types/node`, a compile-time stub, moves from 26.6.1 to
    26.6.2; no source file or Rust crate changed.

## [0.24.4] — 2026-09-15

### Changed

- **No behavior change: the weekly dependency updates.**
  - Effect: `@types/node`, a compile-time stub, moves from 26.4.1 to
    26.6.1.
  - Effect: five transitive Rust crates are updated: `bitflags` to 2.13.2,
    `crossbeam-utils` to 0.8.23, `mio` to 1.2.3, `smallvec` to 1.16.1 and
    `syn` to 3.0.5.

## [0.24.3] — 2026-09-09

### Fixed

- **`opensipsLsp.versionInHints` and `opensipsLsp.opensipsVersion` now
  take effect.**
  - Was: the extension never passed either setting to the server, so
    setting one changed nothing.
  - Do: set them in Settings instead of the
    `OPENSIPS_LSP_VERSION_IN_HINTS` and `OPENSIPS_LSP_VERSION` environment
    variables.
  - Docs: [Admin guide: opensipsVersion](https://github.com/NormB/opensips-lsp/blob/main/docs/ADMIN.md#opensipsversion-string).

## [0.24.2] — 2026-09-09

### Changed

- **No behavior change: one compile-time dependency update.**
  - Effect: `@types/node` moves from 26.4.0 to 26.4.1; nothing that runs
    in the editor changed.

## [0.24.1] — 2026-09-02

### Changed

- **`vscode-languageclient` moves from 10.1.0 to 10.1.1, which carries LSP
  3.18.3.**
  - Effect: diagnostic pull state survives a document being closed and
    reopened quickly, and the client's stream handlers are configurable.
  - Effect: `@types/node`, a compile-time stub, moves from 26.2.0 to
    26.4.0.

## [0.24.0] — 2026-08-26

### Added

- **A core parameter's hover carries its default and its worked example.**
  - Was: the hover of `db_default_url` gave its description and left out
    the URL format in its example.
  - Effect: all 97 parameters carry their example and 70 carry a default.
- **The twelve modifiers a `socket` line takes hover and complete.**
  - Effect: `use_workers n`, `reuse_port`, `anycast`, `as ip[:port]`,
    `tag ID`, `frag`, `tos n`, `accept_subdomain` and the three
    proxy-protocol forms are covered.
  - Effect: `listen =` is the same statement in 3.x and works the same
    way.
- **Spellings the manual skips hover as the setting they are.**
  - Effect: `workdir` hovers as `wdir`, `tcpthreshold` as `tcp_threshold`,
    and three more.
- **Names no manual page describes are offered, and say so.**
  - Effect: `memdump`, `memlog` and the calls `xdbg()` and `error()`
    complete; the grammar accepts them and no page documents them.

### Fixed

- **An alias of a call is offered as a call.**
  - Was: every alias was filed under parameters, which offered it where a
    parameter belongs.
  - Limit: no spelling in this release is affected.
- **A markdown list in a hover stays a list.**
  - Was: the twelve `socket` modifiers, one bullet each, were joined into
    a single paragraph.
- **A `#` at column zero inside an example is a configuration comment, not
  a heading.**
  - Was: a section ending at one lost the rest of its example; the shipped
    manual has 42 of them.

## [0.23.0] — 2026-08-26

### Added

- **`$var` and `$avp` hover and complete.**
  - Was: the manual documents these two as prose sections, which the
    harvester read past.
- **Typing `xlog(` offers the log levels, quoted.**
  - Effect: `L_ALERT`, `L_CRIT`, `L_ERR`, `L_WARN`, `L_NOTICE`, `L_INFO`
    and `L_DBG` are read from the source tree's `route.c`.
  - Effect: after `xlog("` they arrive unquoted; a `$` still offers
    pseudo-variables.
  - Limit: the argument is a string; `xlog(L_INFO, ...)` is a syntax
    error.
- **Ctrl+Alt+H turns hovers and completion off and on (Cmd+Alt+H on
  macOS).**
  - Effect: it applies without a restart; the status bar reads
    `OpenSIPS hints off`, and diagnostics are left alone.
  - Do: the setting behind it is `opensipsLsp.assistance`.
  - Docs: [Turning the popups off](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#turning-the-popups-off).

### Fixed

- **A repeated heading in a manual page no longer becomes a second,
  unreachable entry.**
  - Was: every lookup takes the first, so the other sat in completion and
    could never be hovered.
- **A heading with nothing under it is no longer offered.**
  - Was: it hovered a header over an empty body.
- **A capitalized heading still matches its keyword.**
  - Was: `## If` became a name no hover could match, and the keyword
    vanished with no error.

## [0.22.0] — 2026-08-25

### Changed

- **The release is no longer under every hover and completion item.**
  - Was: each hover repeated the release that the status bar and the
    release warnings already name.
  - Do: set `versionInHints` to turn the note back on.
  - Docs: [Admin guide: versionInHints](https://github.com/NormB/opensips-lsp/blob/main/docs/ADMIN.md#versioninhints-boolean).
- **Choosing a release is documented.**
  - Effect: `GETTING_STARTED.md` covers the dropdown, `settings.json`, the
    environment variable `check` reads, and pointing at your own tree.
  - Docs: [Checking against a specific OpenSIPS release](https://github.com/NormB/opensips-lsp/blob/main/docs/GETTING_STARTED.md#checking-against-a-specific-opensips-release).

### Fixed

- **The release note names the catalog it describes and the release in
  use.**
  - Was: it named the release the vendored file carried, whatever
    `opensipsVersion` selected.
  - Limit: core documentation is one vendored artifact at 4.0.1 and does
    not follow `opensipsVersion`; the note says so.

## [0.21.0] — 2026-08-25

### Added

- **Choose the release you run: the catalog covers OpenSIPS 3.5.9, 3.6.8
  and 4.0.1.**
  - Do: pick the release with `opensipsLsp.opensipsVersion`, a dropdown of
    the releases the shipped catalog covers.
  - Effect: a warning names that release, and the releases where the
    parameter exists, as in "it exists in 3.5.9, 3.6.8".
  - Docs: [Admin guide: opensipsVersion](https://github.com/NormB/opensips-lsp/blob/main/docs/ADMIN.md#opensipsversion-string).
- **The status bar names the release in use.**
  - Effect: it shows the catalog `modparam` lines are checked against,
    such as `OpenSIPS 4.0.1`, or the configured source tree.

### Changed

- **`modparam` parameters come from each module's own C table.**
  - Effect: 189 parameters the documentation never mentions complete and
    hover, such as `rtpengine`'s `extra_failover_error`.
  - Effect: 25 entries OpenSIPS has never accepted are gone, including
    `tls_mgm`'s `cipher_list_col`.
- **`opensips-lsp check` runs the same `modparam` check the editor does.**
  - Was: a CI job or git hook ran only the analyzer, so an unknown
    parameter passed there.
  - Effect: `check` reports warnings it did not report before.

### Fixed

- **An include inside a block comment is followed, because OpenSIPS
  follows it.**
  - Was: the routes those files define were invisible, and every call
    into them was reported undefined.
  - Effect: a directive written after a statement on the same line, which
    OpenSIPS ignores, is no longer followed.
- **`!!include_file` is no longer shown as valid.**
  - Was: the grammar colored it as an include; OpenSIPS rejects it as a
    syntax error.
  - Limit: `#!include_file` is a comment and is still reported as having
    no effect.
- **Two warnings no longer have a gap in the middle of the sentence.**
  - Was: a line continuation kept its indentation inside the message.

## [0.20.1] — 2026-08-25

### Fixed

- **A fragment gets the language whatever it is called.**
  - Was: the extension asked the server only about files named `*.cfg`,
    so `include/*.inc` fragments got no colors, completion or diagnostics.
  - Effect: any plain-text file is asked about; one nothing includes is
    left alone.
- **The workspace sweep looks for `.inc` and `.m4` files too.**
  - Was: it collected `*.cfg` only, so fragments in other trees resolved
    no root and were analyzed alone.
  - Limit: it is a search for configurations, not a read of every file in
    the folder.
- **52 wrong `modparam` warnings on one real configuration are gone.**
  - Was: a `#` comment inside an example closed the Exported Parameters
    section, so the parameters below it were never harvested.
  - Effect: rtpengine, presence, dispatcher, registrar, tracer, prometheus
    and eleven other modules regain their parameters.
  - Effect: `db_url(str)`, the type written with no space, is harvested
    under its name.
- **A heading that lists several parameters documents each of them.**
  - Was: sixteen parameters of `osp` and `tls_mgm` were unreachable, such
    as `server_domain, client_domain` under one heading.
- **Seven more modules complete after `loadmodule "`.**
  - Was: `xml`, `event_datagram`, `db_perlvdb`, `presence_mwi`,
    `presence_xcapdiff`, `tls_openssl` and `auth_web3` export nothing and
    were dropped from the catalog.
- **A module whose documentation could not be read no longer causes
  `modparam` warnings.**
  - Was: `auth_web3`'s README is in a shape the harvester does not read,
    and its empty parameter list was taken as complete.

## [0.20.0] — 2026-08-25

### Added

- **An included file opened on its own gets the language.**
  - Effect: the extension asks the server whether anything includes the
    file; files another extension claims are left alone.
  - Do: turn it off with `opensipsLsp.associateIncludedFiles`.
  - Limit: the extension still does not claim every `.cfg` file.
- **An included file is analyzed as part of its root.**
  - Effect: routes, modules and defines of the parent are in scope, so
    they are not reported as undefined.
  - Effect: `opensips -C` runs on the root, and each error goes to the
    file it names.
  - Do: open the folder, not the single file; the root is found by reading
    the configs under the workspace folders.
  - Limit: a workspace past 500 configs, or a config too large or
    unreadable, is reported in the output channel.
  - Docs: [An included file opened on its own](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#an-included-file-opened-on-its-own).
- **Navigation spans the whole configuration from inside a fragment.**
  - Effect: go to definition, references, rename, call hierarchy,
    workspace symbols, reference counts and route completion cross the
    include boundary.
- **`opensips-lsp check` finds the root the same way the editor does.**
  - Effect: a correct split configuration no longer fails under
    `--strict`.
- **New request `opensips/analysisRoot` for editors other than VS Code.**
  - Effect: it returns the root's URI for a document, or `null` when the
    document is a program in its own right.
- **A file included behind a `#!ifdef` is part of the configuration.**
  - Effect: OpenSIPS reads it either way, verified against the 4.0.1
    binary; Kamailio compiles such includes out.

### Fixed

- **A failed `opensips -C` run that reported no position no longer names
  an empty file.**
  - Was: the message began `check failed in , line 1:`, a file and a line
    the parser never gave.

## [0.19.2] — 2026-08-24

### Changed

- **Documentation only: the pages state what the extension does with your
  configuration.**
  - Effect: the listing, the README and `SECURITY.md` state that the
    server links no HTTP client and runs only your `opensips` binary.
  - Effect: `SECURITY.md` puts any path by which configuration content,
    file paths or environment values reach the network in scope.
  - Limit: `trace.server` echoes cfg text into the editor's output
    channel, and the editor's own telemetry is outside the server's
    control.
  - Docs: [Data handling](https://github.com/NormB/opensips-lsp/blob/main/SECURITY.md#data-handling).

## [0.19.1] — 2026-08-24

### Fixed

- **The tree-sitter grammar can be built by the editors it is offered
  to.**
  - Was: the generated `src/parser.c` was in no commit, and
    nvim-treesitter, Helix and Zed build from that file.
  - Limit: nothing changes for VS Code users.
- **The Zed guide points at the public repository directly.**
  - Was: it cloned the repository to give the grammar directory a git
    root of its own, which Zed does not need.
  - Effect: Zed takes a `path` for a grammar in a subdirectory, as do
    Helix (`subpath`) and nvim-treesitter (`location`).
  - Docs: [Zed setup](https://github.com/NormB/opensips-lsp/blob/main/docs/ZED.md).

## [0.19.0] — 2026-08-24

### Added

- **`#!` directives are flagged as having no effect.**
  - Effect: OpenSIPS has no preprocessor, so `#!ifdef USE_TCP` guards
    nothing and `#!define X 5060` binds nothing.
  - Limit: only keywords Kamailio's lexer defines are flagged; shebangs,
    ordinary comments and directives inside block comments are left alone.
  - Do: toggle with `opensipsLsp.diagnostics.analyzer`.

### Fixed

- **The documentation no longer describes `#!` lines as preprocessor
  directives.**
  - Was: `docs/FEATURES.md` said they were processed ahead of the parser,
    which is true of the Kamailio server.

## [0.18.1] — 2026-08-24

### Added

- **Every editor is documented, not just VS Code.**
  - Effect: `docs/EDITORS.md` has worked examples for Neovim, coc.nvim,
    Helix, Emacs, Vim, Sublime Text, Kate and JetBrains (via LSP4IJ).
  - Effect: it also covers `check` in CI or a git hook with no editor.
  - Docs: [Editor and tool setup](https://github.com/NormB/opensips-lsp/blob/main/docs/EDITORS.md).
- **Zed has a step-by-step guide.**
  - Effect: `docs/ZED.md` builds the small WebAssembly extension Zed needs
    to run a language server.
  - Docs: [Zed setup](https://github.com/NormB/opensips-lsp/blob/main/docs/ZED.md).

### Fixed

- **The editor examples match the files the extension claims.**
  - Was: they matched `opensips.cfg` only, told Helix to claim every
    `.cfg` file, and presented `opensipsSrc` as the way to get
    documentation.
  - Limit: documentation only; the server behaves exactly as 0.18.0 did.

## [0.18.0] — 2026-08-24

### Added

- **Every module's functions and parameters ship with the extension.**
  - Effect: 186 modules, 352 functions and 1300 parameters harvested from
    OpenSIPS 4.0.1 complete with no source tree configured.
  - Limit: module functions appear only in a config that `loadmodule`s
    them; the list is what 4.0.1 documents, not what your system has.
  - Do: set `opensipsSrc` and your own tree replaces the built-in
    catalogs; the two are never merged.
  - Docs: [Documentation before you configure anything](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#documentation-before-you-configure-anything).

### Fixed

- **Hover and signature help read the syntax before choosing an entry.**
  - Was: a global `log_level=2` hovered as a module's `log_level`
    parameter, and a module not loaded could shadow the core language.
  - Effect: `log_level=2` is the core parameter, and
    `modparam("opentelemetry", "log_level", 2)` is that module's.

## [0.17.1] — 2026-08-24

### Fixed

- **The getting-started guide states the file-association rule
  correctly.**
  - Was: it said a config must be named `opensips.cfg` "or end in
    `.cfg`"; `opensips-proxy.cfg` is claimed and a plain `foo.cfg` is not.
- **`README.md` and the guide say the core language is documented out of
  the box.**
  - Was: they presented `opensipsSrc` as the only way to get
    documentation, wrong for every core entry since 0.17.0.
  - Limit: documentation only; the server behaves exactly as 0.17.0 did.

## [0.17.0] — 2026-08-24

### Added

- **The core language completes out of the box.**
  - Effect: a catalog harvested from OpenSIPS 4.0.1 ships: 66 functions,
    97 parameters and 119 pseudo-variables.
  - Was: typing `log_` offered nothing until `opensipsSrc` named a source
    tree.
  - Do: set `opensipsSrc` and your own tree wins; hover on a built-in
    entry says which version it came from.
  - Limit: module documentation still needs a source tree.

### Fixed

- **`exit` no longer completes as `exit()`.**
  - Was: with a tree configured it inserted a call snippet for a
    statement written `exit;`.
- **Hover no longer comes back empty for `exit` and similar keywords.**
  - Was: a bare keyword beat the documented entry by arriving first.

## [0.16.0] — 2026-08-24

### Added

- **`opensips*.cfg` is recognized.**
  - Effect: `opensips-tls.cfg` and `opensips-local.cfg` get language
    support with no configuration.
  - Limit: `opensips.cfg` and `*.opensips.cfg` continue to work; the
    generic `.cfg` extension is still not claimed.

### Changed

- **The installers say that a sideloaded extension receives no updates.**
  - Effect: `install.sh`, the PowerShell installer and the Getting Started
    guide state it.
  - Do: re-run the script, or install from the Extensions view so updates
    arrive on their own.

## [0.15.2] — 2026-08-23

### Fixed

- **The marketplace listing describes what the extension does.**
  - Was: formatting, call hierarchy, inlay hints, include links, pull
    diagnostics, watched files and refactorings were shipped and not
    mentioned.
  - Effect: `docs/ADMIN.md` gains the missing `inlayHintParameterNames`
    option.

## [0.15.1] — 2026-08-23

### Fixed

- **The formatter keeps the indentation of continuation lines.**
  - Was: on a 993-line production config it wanted to change 25 lines,
    all wrongly, including the body of a braceless `if`.
  - Effect: a line is re-indented only when the previous code line ended
    a statement.

## [0.15.0] — 2026-08-23

### Added

- **Extract into a route: selected lines move into a `route[EXTRACTED]` of
  their own.**
  - Effect: a `route(EXTRACTED);` call is left behind at the same
    indentation.
  - Limit: it refuses unbalanced braces, a selection covering the block's
    own braces, and one containing `return`.
  - Docs: [Refactorings](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#refactorings).
- **Remove duplicate `loadmodule` lines.**
  - Effect: every occurrence after the first is removed and nothing is
    reordered; a second load of a module is a parse error.

## [0.14.0] — 2026-08-23

### Added

- **Pull diagnostics: `textDocument/diagnostic` and
  `workspace/diagnostic`.**
  - Effect: the workspace sweep reports problems across the project
    without opening a file; an unchanged document comes back `unchanged`.
  - Effect: only root configs appear in the sweep; a config another
    config includes is covered by its root.
  - Effect: pushing stops when the client pulls, so nothing is reported
    twice.
  - Limit: the sweep stops at 500 configs and logs that it did.
  - Docs: [Pull diagnostics](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#pull-diagnostics).

## [0.13.0] — 2026-08-23

### Added

- **Watched files: an include or the documentation tree changing on disk
  is noticed.**
  - Effect: a git checkout, a rebuild or another tool re-checks and
    re-harvests without the buffer being touched.
  - Effect: a re-check driven by a watched file publishes even when the
    result is clean, so a stale warning is cleared.
  - Docs: [Watched files](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#watched-files).

## [0.12.0] — 2026-08-23

### Added

- **Inlay hints label arguments with the parameter names the module
  documentation gives.**
  - Effect: `t_relay("udp", 1)` reads as
    `t_relay(flags: "udp", outbound_proxy: 1)` without the document
    changing.
  - Limit: only calls the catalog knows are hinted, and only as far as
    the signature goes.
  - Do: `opensipsLsp.inlayHints.parameterNames` turns them off, live.
  - Docs: [Inlay hints](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#inlay-hints).

## [0.11.0] — 2026-08-23

### Added

- **Call hierarchy: Shift+Alt+H on a route name opens the route call
  graph.**
  - Effect: it shows who calls `route[X]` and what `route[X]` calls,
    across the include closure.
  - Effect: a route called but defined nowhere is listed as an outgoing
    edge, marked `undefined`.
  - Limit: asking for the callers of a `failure_route` declines; it is
    armed by a module-function string the server does not track.
  - Docs: [Call hierarchy](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#call-hierarchy).

## [0.10.0] — 2026-08-23

### Added

- **Formatting: Shift+Alt+F and format-on-save re-indent an `opensips.cfg`
  by brace depth.**
  - Effect: `textDocument/formatting` and `textDocument/rangeFormatting`
    strip trailing whitespace and follow the editor's tab settings.
  - Effect: braces inside strings and comments do not move the indent
    depth; `#!` directives keep their column.
  - Limit: it rewrites only the leading and trailing whitespace of a
    line; it never joins, splits or reorders lines.
  - Docs: [Formatting](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#formatting).

## [0.9.1] — 2026-08-23

### Fixed

- **The README names features that shipped earlier.**
  - Was: it did not mention include links, `prepareRename`,
    `semanticTokens/range` or live reconfiguration.
- **The settings table in the features page is whole.**
  - Was: a paragraph between its rows put five settings outside the table
    on GitHub and on the extension listing.
- **A note no longer claims every setting change restarts the server.**
  - Was: runtime toggles have applied live since 0.8.0.

## [0.9.0] — 2026-08-23

### Changed

- **The language-server stack moves to `tower-lsp-server`.**
  - Was: `tower-lsp` has had no release since 0.20.0 (August 2023).
  - Effect: no behavior changes; the server builds from 67 dependencies
    instead of 96.

## [0.8.0] — 2026-08-20

### Added

- **`prepareRename`: F2 is blocked off-symbol and pre-selects the route
  name.**
  - Effect: rename starts only on a route name, with the name selected.
- **Include links: `include_file`/`import_file` paths are Ctrl+Click
  document links.**
  - Effect: Ctrl+Click on the path opens the included file.
  - Docs: [Include links](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#include-links).
- **Semantic tokens answer range requests.**
  - Effect: responses are viewport-sized.
- **Runtime toggles apply without a server restart.**
  - Effect: analyzer, snippets, code lens, `maxDiagnostics` and
    `checkTimeoutMs` changes apply live.
- **The documentation harvest shows editor progress.**
  - Effect: a configured tree that yields no documentation raises a
    visible warning.

### Changed

- **`route(x)` resolves only against the main route table (`route[x]`).**
  - Effect: navigation, references, highlight and rename never cross into
    a same-named `failure_route[x]` or other block kinds, matching
    OpenSIPS.
- **The server runs `opensips -C` from the config's own directory, like
  the CLI.**
  - Effect: relative includes and module paths resolve identically
    everywhere.
- **A newer save ends a superseded `-C` run, queued or executing.**
  - Effect: slow parses no longer block fresh results.
- **Only `opensips.cfg` and `*.opensips.cfg` are claimed.**
  - Effect: the generic `.cfg` extension is left alone.
  - Do: use `files.associations` for custom names.

### Fixed

- **Editing module docs in place invalidates the harvest cache.**
  - Was: only directory mtimes were compared.

## [0.7.1] — 2026-08-20

### Fixed

- **Semantic tokens exclude comments the way the analyzer does.**
  - Was: a `#` inside a string hid the pseudo-variables after it, and
    pvars inside `/* ... */` block comments received tokens.

## [0.7.0] — 2026-08-20

### Added

- **Workspace symbols (Ctrl+T).**
  - Effect: route definitions are searchable across every open file and
    its includes.
- **Code lenses show reference counts above named `route` blocks.**
  - Effect: counts span the include closure.
  - Do: the setting is `opensipsLsp.codeLens.references`.
- **Quick fixes for an unknown function and an undefined route.**
  - Effect: one loads the module that exports an `unknown command <f>`
    function; one creates a stub for an undefined `route(x)` target.
- **Catalog-pinned validation of `modparam` parameters.**
  - Effect: parameters not documented by your configured source tree warn
    as you type.
- **Semantic highlighting of route names and pseudo-variables.**
  - Effect: pseudo-variables are highlighted inside double-quoted strings,
    where OpenSIPS interpolates them.
- **CLI check mode: `opensips-lsp check [--strict] [--bin <opensips>]
  <file>...`.**
  - Effect: the same analyzer plus the real parser run in CI pipelines
    and git hooks.
  - Docs: [CLI check mode](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md#cli-check-mode).

## [0.6.1] — 2026-08-20

### Changed

- **Rename accepts only names legal unquoted: an identifier or a number.**
  - Was: renaming to `a.b` or `x-y` produced configs the parser rejects.
- **Keyword completion drops `strlen`, `send_reply` and `subst` and adds
  `break`.**
  - Effect: `strlen` does not exist; `send_reply` belongs to the signaling
    module and `subst` to textops.
- **References and rename operate across the include closure.**
  - Effect: the docs quote the full parser message and state include
    resolution; the admin guide documents every option and environment
    variable.

### Fixed

- **Diagnostic ranges no longer extend one character past the token.**
  - Was: OpenSIPS reports an exclusive end column, and squiggles extended
    one character past the token.
- **Errors inside `include_file` targets appear on the root document at
  the include directive.**
  - Was: they were dropped, so a broken config could render clean.
- **`timer_route[name, interval]` is recognized everywhere.**
  - Effect: outline, folding, duplicate detection and highlighting see it.
- **Single-quoted strings are understood.**
  - Effect: the analyzer, both grammars, and route names and includes
    accept them.
- **The analyzer resolves `route(0)` to the main route and keeps route
  kinds apart.**
  - Effect: `failure_route[x]` no longer satisfies `route(x)`; duplicates
    are tracked per kind.
  - Limit: these warnings are additive; `opensips -C` accepts undefined
    route targets.
- **Signature help keeps nested parameters intact.**
  - Effect: a call such as `json_link($json(a), $json(b))` is read whole.

### Security

- **`serverPath` is restricted in untrusted workspaces, like the checker
  path.**
  - Effect: the setting is restricted there, as the checker path already
    was.

## [0.6.0] — 2026-08-19

### Added

- **Signature help.**
  - Effect: the innermost unclosed call's signature shows on `(` and `,`,
    with the active parameter highlighted.
  - Effect: module exports are looked up first, then core functions.
- **References, rename and highlights for route names.**
  - Effect: Shift+F12 lists every call site and the definition; F2
    renames everywhere and rejects illegal names.
  - Effect: occurrences highlight, with the definition as a write.
- **Include awareness: `include_file`/`import_file` are followed.**
  - Effect: completion sees included modules and routes, and go to
    definition jumps into the include.
  - Effect: open buffers are preferred over disk, cycles are safe, and
    dotted route names resolve whole.
- **Analyzer warnings appear between saves, debounced as you type.**
  - Effect: undefined `route()` targets and duplicate route definitions
    are reported.
  - Do: the setting is `opensipsLsp.diagnostics.analyzer`.
- **Folding for every route-family block.**
  - Effect: the outline and breadcrumbs carry full block extents.

### Changed

- **Completion collapses duplicate labels and replaces a typed `$token`.**
  - Effect: duplicate labels collapse to the most informative item, and
    `route(` completes route names.

### Fixed

- **`maxDiagnostics` applies in the check path.**
  - Was: it was ignored there, shadowed by the output cap.

## [0.5.4] — 2026-08-19

### Changed

- **The display name is "OpenSIPS Routing Script".**
  - Was: the previous names collide with Microsoft's marketplace
    similarity rules.

## [0.5.3] — 2026-08-19

### Changed

- **The display name is "OpenSIPS Language Support".**
  - Effect: the extension is listed under that name.

## [0.5.2] — 2026-08-19

### Changed

- **The full changelog ships with the extension.**
  - Effect: the extension carries this document, and its overview page is
    rewritten.

## [0.5.1] — 2026-08-19

### Changed

- **The OpenSIPS logo mark is the extension icon.**
  - Effect: the extension shows the logo mark as its icon.

## [0.5.0] — 2026-08-19

### Added

- **Snippets: function completions insert tabstop snippets, such as
  `t_relay(│)`.**
  - Effect: 15 static snippets scaffold route blocks,
    `loadmodule`/`modparam`, control flow, `xlog` and `send_reply`.
- **New settings.**
  - Effect: `opensipsLsp.enable`, `opensipsLsp.trace.server`,
    `opensipsLsp.diagnostics.enable` and
    `opensipsLsp.diagnostics.maxProblems` are added.
  - Effect: `opensipsLsp.completion.snippets` and `opensipsLsp.cacheDir`
    are added.
  - Docs: [Features, Settings & Snippets](https://github.com/NormB/opensips-lsp/blob/main/docs/FEATURES.md).

## [0.4.1] — 2026-08-19

### Changed

- **The extension description is cleaned up.**
  - Effect: the description text of the listing changed.

## [0.4.0] — 2026-08-19

### Added

- **Windows support: native server builds for Windows x64 and arm64.**
  - Effect: `win32-x64` and `win32-arm64` extension packages bundle the
    server.
  - Effect: Linux, macOS and Windows are covered, x64 and arm64 each; the
    editor picks its platform package.
  - Do: `install.ps1` is a PowerShell one-command installer.

## [0.3.4] — 2026-08-19

### Changed

- **Open VSX publishing is automated on every release.**
  - Effect: each release is published to Open VSX.

## [0.3.3] — 2026-08-19

### Added

- **macOS server builds: arm64 native, x64 cross-compiled.**
  - Effect: the server is built for macOS on both architectures.

### Changed

- **The extension identity is `NormB.opensips-lsp`.**
  - Effect: the extension is installed and found under that identifier.

## [0.3.0] — 2026-08-19

### Added

- **Platform extension packages carry the `opensips-lsp` server inside.**
  - Effect: installing the extension is enough; there is no separate
    download.
- **Diagnostics stay off in untrusted folders.**
  - Effect: checking a config loads its modules, so an untrusted folder
    gets no diagnostics.
  - Effect: settings changes restart the server live.
- **A one-command installer and the Getting Started guide.**
  - Effect: the extension installs with one command, and a guide covers
    first use.
  - Docs: [Getting Started](https://github.com/NormB/opensips-lsp/blob/main/docs/GETTING_STARTED.md).
- **The grammar is proven on the OpenSIPS example corpus.**
  - Effect: every syntactically valid example config shipped with OpenSIPS
    parses.

### Changed

- **Diagnostics are hardened against stale and oversized checker output.**
  - Effect: failed or timed-out checks clear stale squiggles, and
    diagnostics are versioned against the buffer they were computed for.
  - Effect: checker output and message lengths are bounded, and
    diagnostics attach by canonical path, safe with symlinks.
  - Effect: harvested documentation is sanitized before rendering.

## [0.2.0] — 2026-08-19

### Added

- **Tree-sitter grammar for error-tolerant highlighting.**
  - Effect: highlighting continues through syntax errors.
- **Full OpenSIPS 4.x core-language coverage.**
  - Effect: core functions, core parameters, and pseudo-variables after
    `$` complete.
  - Effect: coverage is proven against real 4.x and 3.6.x trees and
    binaries.
- **Documentation-catalog caching per source tree.**
  - Effect: a source tree's documentation catalog is cached.

### Fixed

- **Positions are correct on multibyte lines.**
  - Effect: UTF-16 mapping is applied end to end.

## [0.1.0] — 2026-08-19

### Added

- **Initial release.**
  - Effect: diagnostics via `opensips -C`, and context-sensitive
    completion of modules, parameters and functions of loaded modules.
  - Effect: hover documentation, go to definition for routes, and document
    symbols.
