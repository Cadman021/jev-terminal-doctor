pub mod interceptor;

/// یک خروجی خام که از ترمینال گرفته شده، پیش از تحلیل
/// (فعلاً رزرو برای overlay زنده؛ با allow تا CI سبز بماند)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RawOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub command: String,
}
