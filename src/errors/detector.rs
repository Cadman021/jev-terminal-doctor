use once_cell::sync::Lazy;
use regex::Regex;

use super::{ErrorFinding, Toolchain};

// Common error patterns for several toolchains. These are only a starting
// point — the goal is to forward only genuinely error-related blocks to
// ai::client instead of spending API budget on every output line.
static RUSTC_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"error(?:\[E\d+\])?:.*\n\s*-->\s*(?P<file>[^:]+):(?P<line>\d+):\d+").unwrap()
});

static CARGO_TEST_FAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"test .* \.\.\. FAILED").unwrap());

static NODE_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:TypeError|ReferenceError|SyntaxError):.*\n\s*at .*\((?P<file>[^:]+):(?P<line>\d+):\d+\)")
        .unwrap()
});

static PYTHON_TRACEBACK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"File "(?P<file>[^"]+)", line (?P<line>\d+).*\n.*\n(?P<err>\w+Error:.*)"#).unwrap()
});

static GO_BUILD_ERROR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?P<file>[\w./-]+\.go):(?P<line>\d+):\d+: .*").unwrap());

/// Scan a raw output buffer for error patterns.
/// Returns an `ErrorFinding` for the next stage
/// (context::collector + ai::client) on a confident match.
///
/// Note: `raw_snippet` is only the last ~4KB of the buffer, not all of
/// it — this bounds API cost and information leakage.
pub fn scan(buffer: &str) -> Option<ErrorFinding> {
    // Strip ANSI before matching (cargo/pytest output is colored).
    let cleaned = strip_ansi_escapes::strip_str(buffer);
    let buf = cleaned.as_str();
    let snippet = tail_snippet(buf, 4000);

    if let Some(caps) = RUSTC_ERROR.captures(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Rust,
            raw_snippet: snippet,
            file_hint: caps.name("file").map(|m| m.as_str().to_string()),
            line_hint: caps.name("line").and_then(|m| m.as_str().parse().ok()),
        });
    }

    if CARGO_TEST_FAIL.is_match(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Rust,
            raw_snippet: snippet,
            file_hint: None,
            line_hint: None,
        });
    }

    if let Some(caps) = NODE_ERROR.captures(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Node,
            raw_snippet: snippet,
            file_hint: caps.name("file").map(|m| m.as_str().to_string()),
            line_hint: caps.name("line").and_then(|m| m.as_str().parse().ok()),
        });
    }

    if let Some(caps) = PYTHON_TRACEBACK.captures(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Python,
            raw_snippet: snippet,
            file_hint: caps.name("file").map(|m| m.as_str().to_string()),
            line_hint: caps.name("line").and_then(|m| m.as_str().parse().ok()),
        });
    }

    if let Some(caps) = GO_BUILD_ERROR.captures(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Go,
            raw_snippet: snippet,
            file_hint: caps.name("file").map(|m| m.as_str().to_string()),
            line_hint: caps.name("line").and_then(|m| m.as_str().parse().ok()),
        });
    }

    None
}

/// Return the last `max_chars` characters of the buffer (safe on char boundaries).
fn tail_snippet(buf: &str, max_chars: usize) -> String {
    if buf.len() <= max_chars {
        return buf.to_string();
    }
    let start = buf
        .char_indices()
        .rev()
        .take(max_chars)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    buf[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rustc_error() {
        let sample = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n";
        let finding = scan(sample).expect("should be detected");
        assert_eq!(finding.toolchain, Toolchain::Rust);
        assert_eq!(finding.file_hint.as_deref(), Some("src/main.rs"));
        assert_eq!(finding.line_hint, Some(10));
    }

    #[test]
    fn ignores_normal_output() {
        let sample = "Compiling jev-terminal-doctor v0.1.0\nFinished dev profile\n";
        assert!(scan(sample).is_none());
    }
}
