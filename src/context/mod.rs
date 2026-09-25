pub mod collector;
pub mod project;

use serde::{Deserialize, Serialize};

/// Collected context that will be sent to the AI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectContext {
    pub project_kind: Option<String>, // "cargo", "npm", "python-poetry", ...
    pub relevant_diff: Option<String>,
    pub relevant_file_snippets: Vec<(String, String)>, // (path, content)
}
