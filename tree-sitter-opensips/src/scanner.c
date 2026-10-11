/*
 * External scanner for tree-sitter-opensips: nested block comments.
 *
 * OpenSIPS block comments nest — cfg.lex increments `comment_nest` on
 * every opening delimiter inside a comment and leaves the comment only
 * when it drops back to zero — and no regular expression can count, so
 * the token is matched here.  An unterminated comment produces no token,
 * and the parser reports the error.
 */

#include "tree_sitter/parser.h"

#include <wctype.h>

enum TokenType { BLOCK_COMMENT };

void *tree_sitter_opensips_external_scanner_create(void) { return NULL; }

void tree_sitter_opensips_external_scanner_destroy(void *payload) { (void)payload; }

unsigned tree_sitter_opensips_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}

void tree_sitter_opensips_external_scanner_deserialize(void *payload, const char *buffer,
                                                       unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

bool tree_sitter_opensips_external_scanner_scan(void *payload, TSLexer *lexer,
                                                const bool *valid_symbols) {
  (void)payload;
  if (!valid_symbols[BLOCK_COMMENT]) return false;
  while (iswspace(lexer->lookahead)) lexer->advance(lexer, true);
  if (lexer->lookahead != '/') return false;
  lexer->advance(lexer, false);
  if (lexer->lookahead != '*') return false;
  lexer->advance(lexer, false);
  unsigned depth = 1;
  while (depth > 0) {
    if (lexer->eof(lexer)) return false;
    if (lexer->lookahead == '/') {
      lexer->advance(lexer, false);
      if (lexer->lookahead == '*') {
        lexer->advance(lexer, false);
        depth++;
      }
    } else if (lexer->lookahead == '*') {
      lexer->advance(lexer, false);
      if (lexer->lookahead == '/') {
        lexer->advance(lexer, false);
        depth--;
      }
    } else {
      lexer->advance(lexer, false);
    }
  }
  lexer->result_symbol = BLOCK_COMMENT;
  return true;
}
