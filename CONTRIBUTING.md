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
