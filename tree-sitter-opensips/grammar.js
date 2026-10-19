/**
 * Tree-sitter grammar for the OpenSIPS routing script language
 * (opensips.cfg).  Core coverage: globals, loadmodule/modparam,
 * route-family blocks, control flow, calls, pseudo-variables.
 */

module.exports = grammar({
  name: 'opensips',

  // Block comments NEST: cfg.lex counts `comment_nest` up on every
  // `/*` inside a comment and leaves it only back at zero, which no
  // regular token can express — src/scanner.c matches them.
  externals: $ => [$.block_comment],

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

    // The grammar did not model includes at all, while the server's
    // scanner follows them and the whole split-configuration feature
    // rests on them — so an editor driven by this grammar and the
    // server driven by that scanner disagreed about what the file
    // even contains.
    //
    // Neither `!!` nor `#!` is an include prefix here, and they fail
    // differently.  Put to the pinned 4.0.2 binary with a file that
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
      field('value', choice(
        seq($.socket_value, repeat($.socket_parameter)),
        $.host_value,
        $._expression,
      )),
    ),

    // what may follow a socket definition (cfg.y socket_def_param):
    // `socket = udp:192.0.2.10:5060 use_workers 4 as 192.0.2.1:5060`
    socket_parameter: $ => choice(
      choice('anycast', 'ANYCAST', 'frag', 'FRAG', 'reuse_port', 'REUSE_PORT',
        'accept_subdomain', 'ACCEPT_SUBDOMAIN'),
      // ALLOW_PROXY_PROTOCOL, SEND_PROXY_PROTOCOL, PROXY_PROTOCOL
      token(prec(1, /((allow|ALLOW|send|SEND)[-_])?(proxy|PROXY)([-_](protocol|PROTOCOL))?/)),
      seq(choice('use_workers', 'USE_WORKERS'), $.number),
      seq(choice('tos', 'TOS'), $.number),
      seq(choice('as', 'AS'), $.socket_address),
      seq('tag', $.identifier),
      seq(choice('use_auto_scaling_profile', 'USE_AUTO_SCALING_PROFILE'), $.identifier),
    ),

    // the advertised address after `as`: host or IP, optional port
    socket_address: _ => token(/[A-Za-z0-9_.\[\]*-]+(:[0-9]+(-[0-9]+)?)?/),

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
      // the flattener splices an include wherever it stands, so one
      // inside a route block is legal (accepted by opensips -C 4.0.2)
      $.include,
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

    // `return -1;` and `return $var(x);` need no parentheses (cfg.y
    // RETURN snumber / RETURN script_var); `return($var(x), ...)`
    // carries more than one value
    keyword_statement: $ => seq(
      choice('exit', 'drop', 'return', 'break'),
      optional(choice(
        seq('(', ')'),
        $._expression,
        seq('(', $._expression, repeat1(seq(',', $._expression)), ')'),
      )),
      ';',
    ),

    // cfg.y assignop
    assignment_statement: $ => seq(
      field('target', $.pseudo_variable),
      choice('=', ':=', '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^='),
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
      $.string_concatenation,
      $.ip_address,
      $.number,
      $.identifier,
    ),

    // cfg.y folded_string: adjacent strings are one string
    string_concatenation: $ => prec.left(seq($.string, repeat1($.string))),

    // cfg.y ipnet: an IPv4 address with an optional netmask length,
    // compared as a network (`$si == 198.51.100.0/24`)
    ip_address: _ => token(prec(2, /[0-9]{1,3}(\.[0-9]{1,3}){3}(\/[0-9]{1,2})?/)),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    call_expression: $ => prec(2, seq(
      field('function', $.identifier),
      '(', optional(seq($._argument, repeat(seq(',', $._argument)))), ')',
    )),

    _argument: $ => $._expression,

    binary_expression: $ => {
      // cfg.y: OR AND below BOR BAND BXOR BLSHIFT BRSHIFT below the
      // arithmetic operators; a comparison is one exp_cond, which a
      // NOT (level 3, see unary_expression) wraps whole
      const table = [
        ['||', 1], ['&&', 2],
        ['==', 4], ['!=', 4], ['=~', 4], ['!~', 4],
        ['<', 5], ['>', 5], ['<=', 5], ['>=', 5],
        ['|', 6], ['&', 6], ['^', 6], ['<<', 6], ['>>', 6],
        ['+', 7], ['-', 7],
        ['*', 8], ['/', 8], ['%', 8],
      ];
      return choice(...table.map(([op, p]) =>
        prec.left(p, seq($._expression, op, $._expression))));
    },

    // NOT is `!` or `not` (cfg.lex), and in cfg.y `NOT exp` negates a
    // whole comparison: `!$var(a) == 1` is `!($var(a) == 1)`, while
    // `!a && b` is `(!a) && b`.  BNOT `~` and a sign bind tightest.
    unary_expression: $ => choice(
      prec(3, seq(choice('!', 'not'), $._expression)),
      prec(9, seq(choice('-', '~'), $._expression)),
    ),

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

    // Both quote styles may span lines (cfg.lex STRING1/STRING2 take
    // {CR}); only the double-quoted one has escapes.
    string: _ => token(choice(
      seq('"', repeat(choice(/[^"\\]/, /\\(.|\n)/)), '"'),
      seq("'", repeat(/[^']/), "'"),
    )),

    number: _ => /0[xX][0-9a-fA-F]+|[0-9]+/,

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
  },
});
