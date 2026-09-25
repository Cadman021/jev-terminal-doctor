pub mod interceptor;

/// Raw output captured from the terminal, before analysis.
/// (Currently reserved for the live overlay; allow keeps CI green.)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RawOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub command: String,
}
