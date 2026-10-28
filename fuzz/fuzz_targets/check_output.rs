//! Any text as the output of `opensips -C`, with any exit status from the
//! first byte. No range is empty or reversed, no message is empty or
//! passes the cap editors are protected by, and a failed check always
//! reports something.
#![no_main]
#![deny(clippy::unwrap_used)]

use libfuzzer_sys::fuzz_target;
use opensips_lsp::diag;

/// The cap `diag` puts on a message, plus the ellipsis it appends.
const MESSAGE_MAX: usize = 500 + '\u{2026}'.len_utf8();

fuzz_target!(|data: &[u8]| {
    let Some((&rc, rest)) = data.split_first() else {
        return;
    };
    let Ok(output) = std::str::from_utf8(rest) else {
        return;
    };
    let rc = i32::from(rc) - 128;
    let found = diag::parse_check_output(output, rc);
    if rc != 0 {
        assert!(!found.is_empty(), "a failed check reported nothing");
    }
    for d in &found {
        assert!(d.col_end > d.col_start, "an empty or reversed range");
        assert!(d.message.len() <= MESSAGE_MAX, "a message past the cap");
        assert!(!d.message.trim().is_empty(), "an empty message");
    }
});
