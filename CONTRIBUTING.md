# Contributing to opensips-lsp

## The one hard rule: TDD

Every behavior change lands test-first: write the failing test, watch
it fail, implement, watch it pass. PRs that add behavior without new
tests will be asked to add them. Parsing-adjacent code (cfg text,
module docs, `opensips -C` output) must also cover adversarial input:
empty, NUL bytes, backslashes, truncated constructs.

## Local workflow

```sh
cargo test                     # full suite, includes the stdio LSP e2e
cargo clippy --all-targets     # CI enforces -D warnings
cargo fmt
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps   # missing_docs is deny
```

The real-tree harvest test is opt-in:
`OPENSIPS_LSP_TEST_TREE=/path/to/opensips cargo test`.

## PRs

- Feature branches, one problem per PR, linear history (squash or
  rebase merges).
- `docs/ADMIN.md` documents every user-facing option and its
  structure is enforced by a test — update it when options change.
- The VS Code client (`client/`) compiles in CI with `tsc`; keep its
  settings in sync with the server's initialization options.

## The ground-truth rule

The real parser is the specification. Any assertion about the
opensips.cfg language — in code, tests, or documentation — must cite
its evidence: a grammar source line (`cfg.y` / `cfg.lex` in the
OpenSIPS tree) or a live capture from `opensips -C`. Test fixtures
that represent parser output are captured verbatim from a real run,
never written from memory. If a claim cannot be evidenced, verify it
against the binary before building on it — the costliest defects in
this project's history were tests faithfully green against a wrong
model of the language.

Two differential gates enforce this mechanically
(`tests/differential_test.rs`, env-gated on
`OPENSIPS_LSP_TEST_TREE`/`OPENSIPS_LSP_TEST_BIN`): the analyzer must
stay silent on every config the real parser accepts, and a rename
applied through the real logic path must yield a config the parser
still accepts.

When a new upstream OpenSIPS release lands, re-run the full audit:
docs claims vs the new binary, grammar assumptions vs the new
cfg.y/cfg.lex, and the differential gates against the new tree —
the ground truth moves.

## Reporting bugs

Report a bug or ask for a feature with the issue forms at
<https://github.com/NormB/opensips-lsp/issues/new/choose>. The "Bug report"
form asks what happened, a minimal `opensips.cfg` or the steps that show it,
your editor and LSP client, and the opensips-lsp and OpenSIPS versions; remove real
addresses, domains and credentials from the configuration first. The
"Feature request" form asks what problem the change would solve. Write
issues in English. Every report is public and searchable at
<https://github.com/NormB/opensips-lsp/issues>.

Do not report a vulnerability in a public issue. Use GitHub's private
vulnerability reporting at
<https://github.com/NormB/opensips-lsp/security/advisories/new>;
[SECURITY.md](SECURITY.md) says what is in scope and how soon a report
is answered.
