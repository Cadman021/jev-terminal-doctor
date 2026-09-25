use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::ai::PatchSuggestion;

/// Render the diff with +/- line coloring (like git diff) in an overlay panel.
// Deferred until the passthrough conflict is resolved; allow keeps CI green.
#[allow(dead_code)]
pub fn render(frame: &mut Frame, area: Rect, patch: &PatchSuggestion) {
    let lines: Vec<Line> = patch
        .unified_diff
        .lines()
        .map(|line| {
            let style = if line.starts_with('+') {
                Style::default().fg(Color::Green)
            } else if line.starts_with('-') {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect();

    let title = format!(
        " Suggested patch for {} — {} ",
        patch.file_path, patch.explanation
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    let paragraph = Paragraph::new(lines).block(block);

    frame.render_widget(paragraph, area);
}
