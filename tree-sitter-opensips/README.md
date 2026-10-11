# tree-sitter-opensips

Tree-sitter grammar for the OpenSIPS routing script language
(`opensips.cfg`) — error-tolerant parsing for editor highlighting,
folding, and structural navigation.

Coverage: comments (block comments nest, as in OpenSIPS), strings
(multi-line, and adjacent strings folded into one), numbers, IPv4
addresses and networks, global assignments including socket
parameters, `include_file`/`import_file` (also inside route blocks),
`loadmodule`/`modparam`, every route-family block,
`if`/`else`/`while`/`for`/`switch`, `return` with or without
parentheses, calls, every operator and assignment operator in
`cfg.y`, and pseudo-variables.

The opensips-lsp test suite parses every `.cfg` in the pinned OpenSIPS
source tree that `opensips -C` does not reject as a syntax error, plus
the fixtures in `tests/grammar/`, and fails on any parse error.

## Files

- `src/parser.c` — generated from `grammar.js`, committed, and checked
  against a fresh `tree-sitter generate` by the test suite.
- `src/scanner.c` — hand-written; matches nested block comments, which
  no regular expression can. Build it together with `src/parser.c`
  (Neovim: `files = { "src/parser.c", "src/scanner.c" }`; Helix and Zed
  pick it up on their own).
- `queries/highlights.scm` and `queries/folds.scm` — highlighting and
  folding, using capture names Neovim, Helix and Zed all understand.

```sh
npx tree-sitter-cli@0.26 generate   # regenerates src/parser.c
npx tree-sitter-cli@0.26 test       # corpus tests in test/corpus/ and
                                    # highlight tests in test/highlight/
```
