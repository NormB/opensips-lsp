# OpenSIPS LSP Server

## Admin Guide

### Overview

`opensips-lsp` is a Language Server Protocol server for the OpenSIPS
routing script language (`opensips.cfg`), for any LSP-capable editor.
Positions are exchanged in UTF-16 units per the LSP default, correct
on multibyte lines.

The full capability list lives in [`FEATURES.md`](FEATURES.md) and is
checked against the server on every build, so it cannot fall behind
what ships. It is deliberately not repeated here: a second list is a
second thing to forget.

Semantic validation is delegated to OpenSIPS itself: the server runs
`opensips -C -f <file>` and maps the parser's own errors
(file:line:column) to LSP diagnostics, so results are exact for the
OpenSIPS version installed. Editor intelligence (completion, hover)
comes from a documentation catalog harvested from an OpenSIPS source
tree right after initialization (the harvest shows as editor
progress, a readiness log message reports the counts, and a
configured tree yielding no documentation raises a visible warning;
results are cached — see Caching) — the 4.x markdown docs (`modules/*/README.md`) are
the most current and win; docbook (`modules/*/doc/*_admin.xml`) is
the fallback for older trees. Supported and version-proven: OpenSIPS
4.0.x (binary and tree at tag 4.0.1) and 3.6.x (3.6.8).

### Dependencies

#### External Libraries or Applications

The server itself has no runtime library dependencies. Optional but
recommended:

* an `opensips` binary — enables diagnostics (`-C`). Without it (or
  with the parameter set empty) diagnostics are disabled while all
  other features keep working.
* an OpenSIPS source tree — enables completion and hover
  documentation.

### Exported Parameters

The parameters below are passed as LSP `initializationOptions` (see
the editor guides in `docs/EDITORS.md`); each has an environment
fallback for clients that cannot pass options.

#### opensipsPath (string)

Path to the `opensips` binary used for `-C` diagnostics. Set to the
**empty string** to disable diagnostics entirely — see the Security
note below.

*Default value is `opensips` (PATH lookup). Environment fallback:
`OPENSIPS_LSP_BIN`.*

```json title="Set opensipsPath parameter"
{ "opensipsPath": "/usr/local/sbin/opensips" }
```

#### opensipsSrc (string)

OpenSIPS source tree to harvest module documentation from. When
unset, completion is limited to core keywords and route names, and
module hover is unavailable.

*Default value is unset. Environment fallback: `OPENSIPS_LSP_SRC`.*

```json title="Set opensipsSrc parameter"
{ "opensipsSrc": "/opt/src/opensips" }
```

#### versionInHints (boolean)

Whether built-in documentation repeats the release it came from, under
every hover and completion item.

Off by default. The release is on the status bar the whole time a
config is open, and every warning that turns on it names it, so a
hover saying it again is the same fact a third time. Turn it on if you
read hovers in isolation or paste them elsewhere and want the
provenance travelling with the text.

The note distinguishes the two catalogues, because they are pinned
differently: module documentation follows `opensipsVersion`, while
core documentation is a single vendored artefact and names its own
release whatever you have selected.

*Default value is `false`. Environment fallback:
`OPENSIPS_LSP_VERSION_IN_HINTS`.*

```json title="Set versionInHints parameter"
{ "versionInHints": true }
```

#### opensipsVersion (string)

Which built-in OpenSIPS release to check `modparam` names against.
What a module exports moves between releases, so a configuration that
is correct on one can look wrong when judged against another.

In VS Code and VSCodium this is a dropdown listing exactly the
releases the shipped catalogue can answer for, so the value cannot
be mistyped. Editing settings as JSON, it accepts any release the
built-in catalogue covers — currently
`3.5.9`, `3.6.8` and `4.0.1`. An unrecognised value is reported and
the newest is used, rather than silently checking against a release
you did not ask for.

Ignored when `opensipsSrc` is set: your own tree is exact for your
build, and this is a choice among the ones shipped here.

*Default value is the newest release the catalogue covers.
Environment fallback: `OPENSIPS_LSP_VERSION`.*

```json title="Set opensipsVersion parameter"
{ "opensipsVersion": "3.6.8" }
```

#### checkTimeoutMs (integer)

Upper bound, in milliseconds, on one `opensips -C` run. A run that
exceeds it is killed and reported via a client log message.

*Default value is `10000`. Environment fallback:
`OPENSIPS_LSP_CHECK_TIMEOUT_MS`.*

```json title="Set checkTimeoutMs parameter"
{ "checkTimeoutMs": 3000 }
```

#### analyzerDiagnostics (boolean)

Enable the fast analyzer pass between saves: undefined `route()`
targets and duplicate route definitions are flagged as you type
(debounced; tune with `OPENSIPS_LSP_ANALYZER_DEBOUNCE_MS`, default
300 ms). These warnings are additive — `opensips -C` itself does not
detect undefined route targets (they fail at runtime).

Default: `true`.

Example:

```json
{ "analyzerDiagnostics": false }
```

#### assistance (boolean)

Answer hovers and completion at all. Turn it off to read a
configuration without popups appearing over it — walking a
colleague's file, or presenting one — and on again the same way.

In VS Code the toggle is bound to <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+
<kbd>H</kbd> (<kbd>Cmd</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> on macOS),
and the status bar reads `OpenSIPS hints off` while it is off, so a
silent editor is never mistaken for a broken one. It takes effect
immediately: no restart, and no reopening the file.

Diagnostics are not part of it. Whether a configuration is valid is
not noise while reading, and `analyzerDiagnostics` and
`diagnostics.enable` already switch those separately.

Default: `true`.

Example:

```json
{ "assistance": false }
```

#### codeLensReferences (boolean)

Show a reference-count code lens above every named `route` block
(counted across the include closure).

Default: `true`.

Example:

```json
{ "codeLensReferences": false }
```

#### inlayHintParameterNames (boolean)

Draw the parameter name from the documentation before each argument
of a documented call, so `t_relay("udp", 1)` reads as
`t_relay(flags: "udp", outbound_proxy: 1)` without the document
changing. Only calls the catalogue knows are hinted, and the editor
asks for the visible range only.

Default: `true`.

Example:

```json
{ "inlayHintParameterNames": false }
```

#### maxDiagnostics (integer)

Bound on the diagnostics published per file.

*Default value is `100`.*

```json title="Set maxDiagnostics parameter"
{ "maxDiagnostics": 50 }
```

#### snippetCompletions (boolean)

Insert function completions as tabstop snippets.

*Default value is `true`.*

```json title="Set snippetCompletions parameter"
{ "snippetCompletions": false }
```

#### cacheDir (string)

Documentation-catalog cache directory.

*Default value is the platform cache dir. Environment fallback:
`OPENSIPS_LSP_CACHE_DIR`.*

```json title="Set cacheDir parameter"
{ "cacheDir": "/var/cache/opensips-lsp" }
```

### Caching

Harvest results are cached per source tree under
`$XDG_CACHE_HOME/opensips-lsp` (or `~/.cache/opensips-lsp`), keyed by
a fingerprint of the tree's path plus a manifest (path, size, mtime)
of every file the harvest reads — module READMEs, docbook admin
pages, and the core manual pages. Editing, adding, or removing any of
them invalidates the cache automatically; a schema version folded
into the key also invalidates caches written by older server builds.
The readiness log message says `, cached` on a hit. Override the
location with the `OPENSIPS_LSP_CACHE_DIR` environment variable
(env-only knob); delete the directory to force a re-harvest.

### Data handling

Nothing the server reads leaves the machine. No HTTP client is linked
into the binary, it opens no sockets, and it speaks JSON-RPC to the
editor over stdin and stdout. There is no telemetry, no analytics, no
crash reporting and no update check, and no language model is involved
at any point: hover and completion text is parsed from OpenSIPS's own
documentation on disk rather than generated.

Everything it touches is local:

- **Read** — the open configuration and every file its
  `include_file`/`import_file` closure names, plus the configured `opensipsSrc` tree.
- **Written** — the documentation catalog cache described under
  Caching above. It holds harvested documentation only; configuration
  text is never written to it.
- **Executed** — the `opensipsPath` binary, once per check, under the
  constraints described under Security below.

Two caveats matter where configuration content is sensitive:

- `trace.server` set to `messages` or `verbose` writes the LSP traffic
  — which carries the full text of every open configuration — into the
  editor's output channel. It stays on the machine, but it is the one
  place configuration text lands in a log that is easy to attach to a
  bug report. The default is `off`.
- The editor is a separate trust domain. Its own telemetry, and any
  extension with access to the buffer, see the configuration whatever
  this server does.

### Security

`opensips -C` **dlopens the modules the configuration loads**, so
their constructors run: opening a configuration from an untrusted
source executes code. Rely on your editor's workspace-trust
mechanism, or set `opensipsPath` to the empty string for untrusted
trees. `-C` runs are serialized (one at a time) and latest-wins per
document — a newer save kills a superseded run still queued or
executing — bounded by `checkTimeoutMs`, with captured output capped at 1 MiB
(override with the `OPENSIPS_LSP_OUTPUT_CAP_BYTES` environment
variable) — a flooding checker is killed and its run discarded.

### Frequently Asked Questions

#### Why do I see no diagnostics?

Either `opensipsPath` is empty/unresolvable (check the editor's LSP
log for the startup warning), or the file is not saved to disk —
diagnostics run against the on-disk file on open and save.

#### Why does completion show no module functions?

`opensipsSrc` is not set, or the module is not `loadmodule`-ed in the
current file: function completion is intentionally limited to loaded
modules. Core functions and parameters also come from the tree
(`docs/manual/`).

#### Completion looks stale after I updated the source tree

Doc edits are picked up automatically (the fingerprint tracks the
harvested files themselves) but only at server start — restart the
server (in VS Code: reload the window) after changing the tree, or
delete the cache directory (see Caching) to be certain.

### License

Dual-licensed under MIT or Apache-2.0, at your option. See
`LICENSE-MIT` and `LICENSE-APACHE` in the repository root.
