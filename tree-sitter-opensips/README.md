# tree-sitter-opensips

Tree-sitter grammar for the OpenSIPS routing script language
(`opensips.cfg`) — error-tolerant parsing for editor highlighting,
folding, and structural navigation.

Coverage (v0): comments, strings, numbers, global assignments,
`loadmodule`/`modparam`, every route-family block, `if`/`else`/
`while`/`for`/`switch`, calls, operators, pseudo-variables.

```sh
tree-sitter generate   # produces src/ (not committed)
tree-sitter test       # corpus tests in test/corpus/
```
