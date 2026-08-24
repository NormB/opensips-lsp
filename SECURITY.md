# Security Policy

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Report privately via GitHub's private vulnerability reporting:
<https://github.com/NormB/opensips-lsp/security/advisories/new>

Include a description, steps to reproduce or proof of concept, an
impact assessment, and (optionally) your name/handle for credit.

## Response Timeline

| Stage | Target |
|-------|--------|
| Acknowledgment | 48 hours |
| Initial assessment | 7 days |
| Fix for critical issues | 30 days |
| Public disclosure | After fix is released |

## Scope

- **The `-C` execution model** — the server runs `opensips -C` on the
  opened file, which dlopens the modules that file loads. Bypasses of
  the documented opt-out (`opensipsPath` empty), or ways to make the
  server run `-C` on files/paths the user did not open, are in scope.
- **The no-network property** — the server links no HTTP client and
  opens no sockets; it speaks to the editor over stdin/stdout. Any
  path by which configuration content, file paths, or environment
  values reach the network is a vulnerability, not a feature request.
- **Parser robustness** — crafted cfg text, docbook/markdown module
  docs, or `opensips -C` output that crashes the server or corrupts
  its responses (all three parsers are fuzz-adjacent surfaces; they
  must fail closed).
- **Subprocess handling** — argument injection into the `opensips`
  invocation, or resource exhaustion that survives the timeout and
  serialization bounds.

## Out of scope

- Vulnerabilities in OpenSIPS itself (report to the OpenSIPS project).
- Code execution caused by opening an untrusted cfg *with diagnostics
  deliberately enabled* — that is the documented trust model; see the
  Security section of `docs/ADMIN.md`.

## Data handling

No configuration content leaves the machine. There is no telemetry,
no analytics, no crash reporting, no update check, and no model in the
product — hover and completion text is parsed from OpenSIPS's own
documentation on disk. The server reads the cfg and its include
closure plus the source tree you configure, writes a documentation
cache under `cacheDir`, and executes exactly one external program:
your `opensips` binary, for `-C` diagnostics. The Data handling
section of `README.md` has the full account, including the
`trace.server` caveat.
