pub mod detector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Toolchain {
    Rust,
    Node,
    Python,
    Go,
    #[allow(dead_code)]
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ErrorFinding {
    pub toolchain: Toolchain,
    pub raw_snippet: String,
    /// File path and line number reported by the compiler/test runner (if parseable).
    pub file_hint: Option<String>,
    pub line_hint: Option<u32>,
}
