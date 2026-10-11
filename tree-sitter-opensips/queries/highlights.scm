; Highlighting for opensips.cfg.  Capture names are the common set
; Neovim, Helix and Zed all map to colours.

(comment) @comment
(block_comment) @comment

(string) @string
(number) @number
(ip_address) @string.special
(socket_value) @string.special
(socket_address) @string.special
(host_value) @string.special

(pseudo_variable) @variable.builtin
(global_assignment
  name: (identifier) @variable)

; route blocks and their names
(route_kind) @keyword
(route_definition
  name: (identifier) @function)

; `route(NAME)` calls a route block; every other call is a function
((call_expression
  function: (identifier) @keyword
  (identifier) @function)
  (#eq? @keyword "route"))
((call_expression
  function: (identifier) @function.call)
  (#not-eq? @function.call "route"))

; the directive keyword is part of the node; its path stays a string
(include) @keyword
(socket_parameter) @keyword

[
  "loadmodule"
  "modparam"
  "if"
  "else"
  "while"
  "for"
  "in"
  "switch"
  "case"
  "default"
  "exit"
  "drop"
  "return"
  "break"
  "not"
] @keyword

[
  "=" ":=" "+=" "-=" "*=" "/=" "%=" "&=" "|=" "^="
  "==" "!=" "=~" "!~" "<" ">" "<=" ">="
  "||" "&&" "!" "~"
  "|" "&" "^" "<<" ">>"
  "+" "-" "*" "/" "%"
] @operator

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," ";" ":"] @punctuation.delimiter
