pub mod client;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchSuggestion {
    pub file_path: String,
    /// دیف پیشنهادی به فرمت unified diff، برای نمایش در tui::diff_view
    pub unified_diff: String,
    pub explanation: String,
}
