use once_cell::sync::Lazy;
use regex::Regex;

use super::{ErrorFinding, Toolchain};

// الگوهای رایج خطا برای چند تولچین. این‌ها فقط نقطه‌ی شروع‌اند —
// هدف این است که قبل از صرف هزینه‌ی API روی هر خط خروجی، فقط بلوک‌های
// واقعاً مربوط به خطا به ai::client فرستاده شوند.
static RUSTC_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"error(?:\[E\d+\])?:.*\n\s*-->\s*(?P<file>[^:]+):(?P<line>\d+):\d+").unwrap()
});

static CARGO_TEST_FAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"test .* \.\.\. FAILED").unwrap());

static NODE_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:TypeError|ReferenceError|SyntaxError):.*\n\s*at .*\((?P<file>[^:]+):(?P<line>\d+):\d+\)").unwrap()
});

static PYTHON_TRACEBACK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"File "(?P<file>[^"]+)", line (?P<line>\d+).*\n.*\n(?P<err>\w+Error:.*)"#).unwrap()
});

static GO_BUILD_ERROR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?P<file>[\w./-]+\.go):(?P<line>\d+):\d+: .*").unwrap());

/// بافر خروجی خام را برای الگوهای خطا اسکن می‌کند.
/// اگر یک match قطعی پیدا شود، یک `ErrorFinding` برمی‌گرداند تا لایه بعدی
/// (context::collector + ai::client) روی آن کار کند.
///
/// نکته: `raw_snippet` فقط ~4KB آخر بافر است، نه کل آن — تا هزینه‌ی API
/// و لو رفتن اطلاعات محدود بماند.
pub fn scan(buffer: &str) -> Option<ErrorFinding> {
    // حذف ANSI قبل از match (خروجی cargo/pytest رنگی است)
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

/// آخرین `max_chars` کاراکتر بافر (مرز ایمن روی char boundary).
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
        let finding = scan(sample).expect("باید تشخیص داده شود");
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
