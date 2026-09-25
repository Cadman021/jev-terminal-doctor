pub mod collector;
pub mod project;

/// بافت جمع‌آوری‌شده‌ای که قرار است به AI فرستاده شود
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    pub project_kind: Option<String>, // "cargo", "npm", "python-poetry", ...
    pub relevant_diff: Option<String>,
    pub relevant_file_snippets: Vec<(String, String)>, // (path, content)
}
