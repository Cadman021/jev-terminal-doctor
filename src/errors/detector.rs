use once_cell::sync::Lazy;
use regex::Regex;

use super::{ErrorFinding, Toolchain};

// Common error patterns for several toolchains. These are only a starting
// point — the goal is to forward only genuinely error-related blocks to
// ai::client instead of spending API budget on every output line.
static RUSTC_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"error(?:\[E\d+\])?:.*\n\s*-->\s*(?P<file>[^:]+):(?P<line>\d+):\d+").unwrap()
});

static RUST_PANIC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"thread '.*?' panicked at (?P<file>[\w./\\-]+\.rs):(?P<line>\d+):\d+").unwrap()
});

static CARGO_TEST_FAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"test .* \.\.\. FAILED").unwrap());

static NODE_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:TypeError|ReferenceError|SyntaxError):.*\n\s*at .*\((?P<file>[^:]+):(?P<line>\d+):\d+\)")
        .unwrap()
});

// tsc classic: src/index.ts:10:5 - error TS2322: ...
static TSC_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<file>[\w./\\-]+\.tsx?):(?P<line>\d+):\d+\s+-\s+error\s+TS\d+:").unwrap()
});

// tsc pretty (dashboard): src/index.ts(10,5): error TS2322: ...
static TSC_PRETTY_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<file>[\w./\\-]+\.tsx?)\((?P<line>\d+),\d+\):\s+error\s+TS\d+:").unwrap()
});

static NODE_CANNOT_FIND: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Error: Cannot find module '[^']+'").unwrap());

static NPM_ERR: Lazy<Regex> = Lazy::new(|| Regex::new(r"npm ERR!").unwrap());

static PYTHON_TRACEBACK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"File "(?P<file>[^"]+)", line (?P<line>\d+).*\n.*\n(?P<err>\w+Error:.*)"#).unwrap()
});

// pytest short summary: FAILED tests/test_x.py::test_y - ...
static PYTEST_FAILED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"FAILED\s+(?P<file>[\w./\\-]+\.py)(::\S*)?\s+-").unwrap());

// pytest assertion body: lines starting with "E   ..."
static PYTEST_ASSERT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^E\s+(AssertionError|assert .*)").unwrap());

static GO_BUILD_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)(?P<file>[\w./\\-]+\.go):(?P<line>\d+):\d+: (?:.*(?:undefined|expected|error|cannot|invalid|missing|declared|redeclared|mismatch|too many|too few|not enough).*)$").unwrap()
});

static GO_TEST_FAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^--- FAIL:\s+\S+").unwrap());

// Maven: [ERROR] src/main/java/App.java:[10,5] ...
static MAVEN_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[ERROR\]\s+(?P<file>[\w./\\-]+\.java):\[(?P<line>\d+),\d+\]").unwrap()
});

// Gradle / javac: src/main/java/App.java:10: error: ...
static JAVAC_ERROR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?P<file>[\w./\\-]+\.java):(?P<line>\d+):\s+error:").unwrap());

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

    if let Some(caps) = RUST_PANIC.captures(buf) {
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

    if let Some(caps) = TSC_ERROR.captures(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Node,
            raw_snippet: snippet,
            file_hint: caps.name("file").map(|m| m.as_str().to_string()),
            line_hint: caps.name("line").and_then(|m| m.as_str().parse().ok()),
        });
    }

    if let Some(caps) = TSC_PRETTY_ERROR.captures(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Node,
            raw_snippet: snippet,
            file_hint: caps.name("file").map(|m| m.as_str().to_string()),
            line_hint: caps.name("line").and_then(|m| m.as_str().parse().ok()),
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

    if NODE_CANNOT_FIND.is_match(buf) || NPM_ERR.is_match(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Node,
            raw_snippet: snippet,
            file_hint: None,
            line_hint: None,
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

    if let Some(caps) = PYTEST_FAILED.captures(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Python,
            raw_snippet: snippet,
            file_hint: caps.name("file").map(|m| m.as_str().to_string()),
            line_hint: None,
        });
    }

    if PYTEST_ASSERT.is_match(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Python,
            raw_snippet: snippet,
            file_hint: None,
            line_hint: None,
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

    if GO_TEST_FAIL.is_match(buf) {
        return Some(ErrorFinding {
            toolchain: Toolchain::Go,
            raw_snippet: snippet,
            file_hint: None,
            line_hint: None,
        });
    }

    if let Some(caps) = MAVEN_ERROR
        .captures(buf)
        .or_else(|| JAVAC_ERROR.captures(buf))
    {
        return Some(ErrorFinding {
            toolchain: Toolchain::Java,
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
    fn detects_rust_panic_with_location() {
        let sample = "thread 'main' panicked at src/main.rs:42:9:\nindex out of bounds";
        let finding = scan(sample).expect("should be detected");
        assert_eq!(finding.toolchain, Toolchain::Rust);
        assert_eq!(finding.file_hint.as_deref(), Some("src/main.rs"));
        assert_eq!(finding.line_hint, Some(42));
    }

    #[test]
    fn detects_tsc_errors() {
        let classic = "src/index.ts:10:5 - error TS2322: Type 'string' is not assignable.\n";
        let finding = scan(classic).expect("classic tsc should be detected");
        assert_eq!(finding.toolchain, Toolchain::Node);
        assert_eq!(finding.file_hint.as_deref(), Some("src/index.ts"));
        assert_eq!(finding.line_hint, Some(10));

        let pretty = "src/app.tsx(7,3): error TS2304: Cannot find name 'x'.\n";
        let finding = scan(pretty).expect("pretty tsc should be detected");
        assert_eq!(finding.file_hint.as_deref(), Some("src/app.tsx"));
        assert_eq!(finding.line_hint, Some(7));
    }

    #[test]
    fn detects_npm_and_missing_module() {
        assert!(scan("npm ERR! code ELIFECYCLE\nnpm ERR! errno 1\n").is_some());
        assert!(scan("Error: Cannot find module 'express'\nRequire stack:\n").is_some());
    }

    #[test]
    fn detects_pytest_failures() {
        let sample = "FAILED tests/test_api.py::test_create - assert 200 == 500\n";
        let finding = scan(sample).expect("pytest FAILED should be detected");
        assert_eq!(finding.toolchain, Toolchain::Python);
        assert_eq!(finding.file_hint.as_deref(), Some("tests/test_api.py"));

        assert!(scan("E   AssertionError: expected 1, got 2\n").is_some());
    }

    #[test]
    fn detects_go_build_and_test_fail() {
        let sample = "main.go:12:5: undefined: Foo\n";
        let finding = scan(sample).expect("go build error should be detected");
        assert_eq!(finding.toolchain, Toolchain::Go);
        assert_eq!(finding.line_hint, Some(12));

        assert!(scan("--- FAIL: TestHandler (0.01s)\n").is_some());
        // Plain log lines mentioning .go must NOT trigger anymore.
        assert!(scan("INFO server.go:10:3: listening on :8080\n").is_none());
    }

    #[test]
    fn detects_maven_and_javac_errors() {
        let maven = "[ERROR] src/main/java/App.java:[10,5] ';' expected\n";
        let finding = scan(maven).expect("maven error should be detected");
        assert_eq!(finding.toolchain, Toolchain::Java);
        assert_eq!(finding.line_hint, Some(10));

        let javac = "src/main/java/App.java:20: error: cannot find symbol\n";
        assert!(scan(javac).is_some());
    }

    #[test]
    fn ignores_normal_output() {
        let sample = "Compiling jev-terminal-doctor v0.1.0\nFinished dev profile\n";
        assert!(scan(sample).is_none());
    }
}
