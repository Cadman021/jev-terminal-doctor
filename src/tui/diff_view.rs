use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::ai::PatchSuggestion;

/// دیف را با رنگ‌بندی خطوط +/- (مثل git diff) در یک پنل overlay رسم می‌کند.
// deferred تا حل تعارض با passthrough؛ allow برای سبز ماندن CI.
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
        " پچ پیشنهادی برای {} — {} ",
        patch.file_path, patch.explanation
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    let paragraph = Paragraph::new(lines).block(block);

    frame.render_widget(paragraph, area);
}
