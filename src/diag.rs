//! `opensips -C` execution and output parsing.

/// Diagnostic severity, mirroring the LSP levels we emit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    /// A parse or load error: the cfg will not start.
    Error,
    /// A non-fatal complaint.
    Warning,
}

/// One diagnostic parsed out of `opensips -C` output.
#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    /// File the parser attributed the error to (may be empty for
    /// the global fallback diagnostic).
    pub file: String,
    /// 0-based.
    pub line: u32,
    /// 0-based start column.
    pub col_start: u32,
    /// Exclusive end column.
    pub col_end: u32,
    /// Mapped severity.
    pub severity: Severity,
    /// Human-readable message (the yyerror tail).
    pub message: String,
}

/// Parse the stderr of `opensips -C -f <file>` into diagnostics.
///
/// Positioned `yyerror` lines carry `file:line:colstart-colend`
/// (1-based; the end column is EXCLUSIVE — cfg.lex's `count()`
/// reports start..start+len); everything else is noise except as a
/// fallback when the check failed without any positioned error.
pub fn parse_check_output(output: &str, rc: i32) -> Vec<Diag> {
    let positioned = regex::Regex::new(
        r"(CRITICAL|ERROR|WARNING):core:yyerror: parse error in (.+?):(\d+):(\d+)-(\d+): ?(.*)",
    )
    .unwrap();
    let mut out = Vec::new();

    for line in output.lines() {
        let Some(c) = positioned.captures(line) else {
            continue;
        };
        let (Ok(l), Ok(cs), Ok(ce)) = (
            c[3].parse::<u32>(),
            c[4].parse::<u32>(),
            c[5].parse::<u32>(),
        ) else {
            continue; // absurd numbers: skip rather than lie
        };
        let severity = match &c[1] {
            "WARNING" => Severity::Warning,
            _ => Severity::Error,
        };
        let col_start = cs.saturating_sub(1);
        // 1-based exclusive end → 0-based exclusive end is ce-1;
        // clamp so a degenerate report still renders one character
        let col_end = ce.saturating_sub(1).max(col_start + 1);
        let msg = truncate_message(c[6].trim());
        out.push(Diag {
            file: c[2].to_string(),
            line: l.saturating_sub(1),
            col_start,
            col_end,
            severity,
            message: if msg.is_empty() {
                "parse error".to_string()
            } else {
                msg
            },
        });
    }

    if out.is_empty() && rc != 0 {
        let generic = regex::Regex::new(r"(?:ERROR|CRITICAL):[^:]*:[^:]*: (.+)").unwrap();
        let msg = output
            .lines()
            .rev()
            .find_map(|l| generic.captures(l).map(|c| truncate_message(c[1].trim())))
            .unwrap_or_else(|| format!("opensips -C failed (rc={rc}) with unparseable output"));
        out.push(Diag {
            file: String::new(),
            line: 0,
            col_start: 0,
            col_end: 1,
            severity: Severity::Error,
            message: msg,
        });
    }
    out
}

/// Editors render diagnostics inline: bound the message so a hostile
/// or broken checker cannot ship megabytes into the client.
const MESSAGE_CAP: usize = 500;

fn truncate_message(msg: &str) -> String {
    if msg.len() <= MESSAGE_CAP {
        return msg.to_string();
    }
    let mut end = MESSAGE_CAP;
    while end > 0 && !msg.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\u{2026}", &msg[..end])
}
