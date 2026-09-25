use anyhow::Result;
use std::path::Path;

use crate::ai::PatchSuggestion;
use crate::context::collector;
use crate::errors::ErrorFinding;
use crate::tui::actions::save_suggestion;

/// Shared pipeline: collect context -> ask provider -> save suggestion.
/// Used by the PTY analyzer, `jev exec`, and `jev check` so all flows
/// behave identically.
pub async fn analyze_and_suggest(root: &Path, finding: &ErrorFinding) -> Result<PatchSuggestion> {
    let ctx = collector::collect(root, finding)?;
    let (provider, _) = crate::ai::client::make_provider();
    let patch = provider.suggest_patch(finding, &ctx).await?;
    save_suggestion(root, &patch)?;
    Ok(patch)
}

/// Human-readable provider label without building a client.
pub fn provider_name() -> &'static str {
    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.trim().is_empty() => "anthropic",
        _ => "mock (ANTHROPIC_API_KEY not set)",
    }
}
