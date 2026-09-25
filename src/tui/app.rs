use crate::ai::PatchSuggestion;

/// Overall TUI state. When a PatchSuggestion is ready, this state moves
/// from `Idle` to `ReviewingPatch` and the diff overlay (diff_view) is
/// shown on top of the normal terminal output.
// Deferred: live overlay fights transparent passthrough; allow until then.
#[allow(dead_code)]
pub enum AppState {
    Idle,
    ReviewingPatch(PatchSuggestion),
    Applying,
    Done,
}

#[allow(dead_code)]
pub struct App {
    pub state: AppState,
}

#[allow(dead_code)]
impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::Idle,
        }
    }

    pub fn show_patch(&mut self, patch: PatchSuggestion) {
        self.state = AppState::ReviewingPatch(patch);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
