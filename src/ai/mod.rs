pub mod client;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchSuggestion {
    pub file_path: String,
    /// Proposed diff in unified diff format, for display in tui::diff_view.
    pub unified_diff: String,
    pub explanation: String,
}
