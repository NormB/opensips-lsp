# OpenSIPS LSP Server

## Admin Guide

### Overview

`opensips-lsp` is a Language Server Protocol server for the OpenSIPS
routing script language (`opensips.cfg`). It provides diagnostics,
completion, hover documentation, go-to-definition and document
symbols to any LSP-capable editor.

Semantic validation is delegated to OpenSIPS itself: the server runs
`opensips -C -f <file>` and maps the parser's own errors
(file:line:column) to LSP diagnostics, so results are exact for the
OpenSIPS version installed. Editor intelligence (completion, hover)
comes from a documentation catalog harvested from an OpenSIPS source
tree at startup — the 4.x markdown docs (`modules/*/README.md`) are
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

### Security

`opensips -C` **dlopens the modules the configuration loads**, so
their constructors run: opening a configuration from an untrusted
source executes code. Rely on your editor's workspace-trust
mechanism, or set `opensipsPath` to the empty string for untrusted
trees. `-C` runs are serialized (one at a time) and bounded by
`checkTimeoutMs`.

### Frequently Asked Questions

#### Why do I see no diagnostics?

Either `opensipsPath` is empty/unresolvable (check the editor's LSP
log for the startup warning), or the file is not saved to disk —
diagnostics run against the on-disk file on open and save.

#### Why does completion show no module functions?

`opensipsSrc` is not set, or the module is not `loadmodule`-ed in the
current file: function completion is intentionally limited to loaded
modules.

### License

Dual-licensed under MIT or Apache-2.0, at your option — the same
terms as sipnab. See `LICENSE-MIT` and `LICENSE-APACHE` in the
repository root.
