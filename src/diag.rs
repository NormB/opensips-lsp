//! `opensips -C` execution and output parsing.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    pub file: String,
    /// 0-based.
    pub line: u32,
    pub col_start: u32,
    /// exclusive.
    pub col_end: u32,
    pub severity: Severity,
    pub message: String,
}

/// Parse the stderr of `opensips -C -f <file>` into diagnostics.
///
/// Positioned `yyerror` lines carry `file:line:colstart-colend`
/// (1-based, inclusive); everything else is noise except as a
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
        let col_end = ce.max(cs).max(1); // inclusive 1-based end == exclusive 0-based
        let msg = c[6].trim();
        out.push(Diag {
            file: c[2].to_string(),
            line: l.saturating_sub(1),
            col_start,
            col_end,
            severity,
            message: if msg.is_empty() {
                "parse error".to_string()
            } else {
                msg.to_string()
            },
        });
    }

    if out.is_empty() && rc != 0 {
        let generic = regex::Regex::new(r"(?:ERROR|CRITICAL):[^:]*:[^:]*: (.+)").unwrap();
        let msg = output
            .lines()
            .rev()
            .find_map(|l| generic.captures(l).map(|c| c[1].trim().to_string()))
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
