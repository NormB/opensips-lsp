<!-- Thanks for contributing to opensips-lsp! Keep this concise. -->

## Summary

<!-- What does this change and why? -->

## Related issues

<!-- e.g. Closes #123 -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor / internal
- [ ] Documentation
- [ ] CI / build / tooling

## Checklist

- [ ] **TDD**: the failing test came first; the PR shows the behavior
      red→green (`cargo test` passes locally, new behavior has new tests,
      adversarial inputs covered where parsing is involved)
- [ ] `cargo clippy --all-targets` is clean (CI builds with `-D warnings`)
- [ ] `cargo fmt` applied
- [ ] Public items documented (`missing_docs` is `deny`; CI runs
      `cargo doc -D warnings`)
- [ ] `docs/ADMIN.md` / `docs/EDITORS.md` updated if options or editor
      behavior changed (ADMIN.md structure is test-enforced)
