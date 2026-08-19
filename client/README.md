# OpenSIPS Configuration (opensips-lsp client)

Language support for `opensips.cfg` routing scripts, backed by the
[opensips-lsp](https://github.com/NormB/opensips-lsp) server:
diagnostics from a real `opensips -C` run, context-sensitive
completion (modules, parameters, functions, pseudo-variables), hover
documentation, go-to-definition for routes, and document symbols.

Install the server binary (see the repository README), then set:

- `opensipsLsp.serverPath` — path to `opensips-lsp`
- `opensipsLsp.opensipsPath` — `opensips` binary for diagnostics
  (empty disables them)
- `opensipsLsp.opensipsSrc` — OpenSIPS source tree for documentation
- `opensipsLsp.checkTimeoutMs` — bound on one `-C` run
