/**
 * Tree-sitter grammar for the OpenSIPS routing script language
 * (opensips.cfg).  Core coverage: globals, loadmodule/modparam,
 * route-family blocks, control flow, calls, pseudo-variables.
 */

module.exports = grammar({
  name: 'opensips',

  extras: $ => [/\s/, $.comment, $.block_comment],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._top_level),

    _top_level: $ => choice(
      $.include,
      $.loadmodule,
      $.modparam,
      $.global_assignment,
      $.route_definition,
    ),

    comment: _ => token(seq('#', /[^\n]*/)),
    block_comment: _ => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),

    // The grammar did not model includes at all, while the server's
    // scanner follows them and the whole split-configuration feature
    // rests on them — so an editor driven by this grammar and the
    // server driven by that scanner disagreed about what the file
    // even contains.
    //
    // Neither `!!` nor `#!` is an include prefix here, and they fail
    // differently.  Put to the pinned 4.0.1 binary with a file that
    // does not exist, so "did it fire" is "did it try to open it":
    //
    //   include_file "x"      opens x
    //   #!include_file "x"    nothing at all — `#` opens a comment
    //   !!include_file "x"    SYNTAX ERROR at 1:1-2; the text reaches
    //                         the lexer, which has no rule for `!!`
    //
    // This grammar previously accepted `!!`, with a comment claiming
    // the real parser read it.  It does not: an editor driven by that
    // grammar showed valid syntax for something OpenSIPS rejects
    // outright.  Kamailio is a different language and its sibling
    // grammar is measured against its own binary, not this one.
    include: $ => seq(
      token(prec(3, choice('include_file', 'import_file'))),
      $.string,
    ),

    loadmodule: $ => seq('loadmodule', $.string),

    modparam: $ => seq(
      'modparam', '(', $.string, ',', $.string, ',', $._expression, ')',
    ),

    global_assignment: $ => seq(
      field('name', $.identifier), '=',
      field('value', choice($.socket_value, $.host_value, $._expression)),
    ),

    // raw socket/listen values: udp:127.0.0.1:5060, tls:eth0:5061,
    // udp:*:5060 — scheme-prefixed, unquoted
    socket_value: _ => token(prec(1, /[A-Za-z][A-Za-z0-9+.-]*:[^\s;#]+/)),

    // bare host/domain global values: alias=sip.example.com
    host_value: _ => token(prec(-1, /[A-Za-z0-9][A-Za-z0-9_-]*(\.[A-Za-z0-9_-]+)+/)),

    route_definition: $ => seq(
      field('kind', $.route_kind),
      // timer_route carries a mandatory interval: [name, N]
      optional(seq(
        '[',
        field('name', choice($.identifier, $.number, $.string)),
        optional(seq(',', field('interval', $.number))),
        ']',
      )),
      field('body', $.block),
    ),

    route_kind: _ => choice(
      'route', 'failure_route', 'onreply_route', 'branch_route',
      'timer_route', 'event_route', 'error_route', 'local_route',
      'startup_route',
    ),

    block: $ => seq('{', repeat($._statement), '}'),

    _statement: $ => choice(
      $.empty_statement,
      $.if_statement,
      $.while_statement,
      $.for_statement,
      $.switch_statement,
      $.block,
      $.assignment_statement,
      $.expression_statement,
      $.keyword_statement,
    ),

    // old documented style closes blocks with `};`
    empty_statement: _ => ';',

    if_statement: $ => prec.right(seq(
      'if', '(', $._expression, ')', $._statement,
      optional(seq('else', $._statement)),
    )),

    while_statement: $ => seq('while', '(', $._expression, ')', $._statement),

    for_statement: $ => seq(
      'for', '(', $.pseudo_variable, 'in', $._expression, ')', $._statement,
    ),

    switch_statement: $ => seq(
      'switch', '(', $._expression, ')',
      '{', repeat(choice($.case_clause, $.default_clause)), '}',
    ),
    case_clause: $ => seq('case', $._expression, ':', repeat($._statement)),
    default_clause: $ => seq('default', ':', repeat($._statement)),

    keyword_statement: $ => seq(
      choice('exit', 'drop', 'return', 'break'),
      optional(seq('(', optional($._expression), ')')),
      ';',
    ),

    assignment_statement: $ => seq(
      field('target', $.pseudo_variable),
      choice('=', '+=', '-='),
      field('value', $._expression),
      ';',
    ),

    expression_statement: $ => seq($._expression, ';'),

    _expression: $ => choice(
      $.call_expression,
      $.binary_expression,
      $.unary_expression,
      $.parenthesized_expression,
      $.pseudo_variable,
      $.string,
      $.number,
      $.identifier,
    ),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    call_expression: $ => prec(2, seq(
      field('function', $.identifier),
      '(', optional(seq($._argument, repeat(seq(',', $._argument)))), ')',
    )),

    _argument: $ => $._expression,

    binary_expression: $ => {
      const table = [
        ['||', 1], ['&&', 2],
        ['==', 3], ['!=', 3], ['=~', 3], ['!~', 3],
        ['<', 4], ['>', 4], ['<=', 4], ['>=', 4],
        ['+', 5], ['-', 5],
        ['*', 6], ['/', 6], ['%', 6],
      ];
      return choice(...table.map(([op, p]) =>
        prec.left(p, seq($._expression, op, $._expression))));
    },

    unary_expression: $ => prec(7, seq(choice('!', '-'), $._expression)),

    pseudo_variable: _ => token(seq(
      '$', optional('$'),
      choice(
        seq(
          /[A-Za-z_][A-Za-z0-9_.]*/,
          optional(seq('(', /(?:[^()\n]|\([^()\n]*\))*/, ')')),
        ),
        seq('(', /(?:[^()\n]|\([^()\n]*\))+/, ')'),
      ),
    )),

    string: _ => token(choice(
      seq('"', repeat(choice(/[^"\\\n]/, /\\./)), '"'),
      seq("'", repeat(choice(/[^'\\\n]/, /\\./)), "'"),
    )),

    number: _ => /0[xX][0-9a-fA-F]+|[0-9]+/,

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
  },
});
