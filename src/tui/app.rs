use crate::ai::PatchSuggestion;

/// وضعیت کلی رابط کاربری TUI. وقتی یک PatchSuggestion آماده می‌شود،
/// این state از `Idle` به `ReviewingPatch` تغییر می‌کند و overlay دیف
/// (diff_view) روی خروجی عادی ترمینال نمایش داده می‌شود.
// deferred: live overlay با passthrough شفاف می‌جنگد؛ تا آن زمان allow.
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
