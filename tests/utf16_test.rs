//! LSP positions are UTF-16 code-unit columns; the analyzer works in
//! bytes. The conversions must be exact on multibyte lines.

use opensips_lsp::analyze::{byte_to_utf16, utf16_to_byte, word_at};

// "é" = 2 bytes / 1 utf16 unit; "😀" = 4 bytes / 2 utf16 units
const LINE: &str = r#"xlog("é😀"); t_relay();"#;

#[test]
fn utf16_to_byte_walks_multibyte_correctly() {
    // ASCII prefix: identical
    assert_eq!(utf16_to_byte(LINE, 0), 0);
    assert_eq!(utf16_to_byte(LINE, 6), 6); // up to the é
    // after é (1 unit, 2 bytes)
    assert_eq!(utf16_to_byte(LINE, 7), 8);
    // after 😀 (2 units, 4 bytes)
    assert_eq!(utf16_to_byte(LINE, 9), 12);
    // t_relay starts at utf16 13 == byte 16
    assert_eq!(utf16_to_byte(LINE, 13), 16);
    // past end clamps to len
    assert_eq!(utf16_to_byte(LINE, 999), LINE.len());
    // inside the surrogate pair: clamp to the char start
    assert_eq!(utf16_to_byte(LINE, 8), 8);
}

#[test]
fn byte_to_utf16_is_the_inverse_on_char_boundaries() {
    assert_eq!(byte_to_utf16(LINE, 0), 0);
    assert_eq!(byte_to_utf16(LINE, 8), 7);
    assert_eq!(byte_to_utf16(LINE, 12), 9);
    assert_eq!(byte_to_utf16(LINE, 16), 13);
    assert_eq!(byte_to_utf16(LINE, 9999), 23); // clamped to line end in units
}

#[test]
fn word_at_via_utf16_position_finds_the_word_after_emoji() {
    let b = utf16_to_byte(LINE, 14); // inside t_relay
    assert_eq!(word_at(LINE, b), Some("t_relay".to_string()));
}

#[test]
fn ascii_lines_are_identity() {
    let l = "modparam(\"tm\", \"fr_timeout\", 3)";
    for i in 0..l.len() {
        assert_eq!(utf16_to_byte(l, i as u32), i);
        assert_eq!(byte_to_utf16(l, i), i as u32);
    }
}

#[test]
fn adversarial_no_panic() {
    for l in ["", "é", "😀", "\u{7f}"] {
        for c in [0u32, 1, 2, 3, 100] {
            let _ = utf16_to_byte(l, c);
            let _ = byte_to_utf16(l, c as usize);
        }
    }
}
