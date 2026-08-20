# OpenSIPS LSP Server

## Admin Guide

### Overview

`opensips-lsp` is a Language Server Protocol server for the OpenSIPS
routing script language (`opensips.cfg`). It provides diagnostics,
context-sensitive completion (modules, parameters, exported and core
functions, pseudo-variables after `$`), hover documentation,
go-to-definition for routes, and document symbols to any LSP-capable
editor. Positions are exchanged in UTF-16 units per the LSP default,
correct on multibyte lines.

Semantic validation is delegated to OpenSIPS itself: the server runs
`opensips -C -f <file>` and maps the parser's own errors
(file:line:column) to LSP diagnostics, so results are exact for the
OpenSIPS version installed. Editor intelligence (completion, hover)
comes from a documentation catalog harvested from an OpenSIPS source
tree right after initialization (a readiness log message reports the
counts; results are cached — see Caching) — the 4.x markdown docs (`modules/*/README.md`) are
the most current and win; docbook (`modules/*/doc/*_admin.xml`) is
the fallback for older trees. Supported and version-proven: OpenSIPS
4.x (master) and 3.6.x (3.6.8).

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
{ "opensipsSrc": "/home/user/src/opensips" }
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
a fingerprint of the tree's path and the modification times of its
`modules/` and `docs/manual/` directories — adding or removing a
module or doc page invalidates the cache automatically. The readiness
log message says `, cached` on a hit. Override the location with the
`OPENSIPS_LSP_CACHE_DIR` environment variable (env-only knob); delete
the directory to force a re-harvest.

### Security

`opensips -C` **dlopens the modules the configuration loads**, so
their constructors run: opening a configuration from an untrusted
source executes code. Rely on your editor's workspace-trust
mechanism, or set `opensipsPath` to the empty string for untrusted
trees. `-C` runs are serialized (one at a time), bounded by
`checkTimeoutMs`, and their captured output is capped at 1 MiB
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

Editing a file inside a module does not bump the directory mtimes the
cache fingerprint watches. Delete the cache directory (see Caching)
or touch `modules/` to force a re-harvest.

### License

Dual-licensed under MIT or Apache-2.0, at your option. See
`LICENSE-MIT` and `LICENSE-APACHE` in the repository root.
