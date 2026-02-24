use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, ViewMode};

pub fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default();
    let sep = Span::styled("  ", desc_style);

    let spans = match app.view_mode {
        ViewMode::Tree => vec![
            Span::styled("\u{2191}\u{2193}", key_style),
            Span::styled(" navigate", desc_style),
            sep.clone(),
            Span::styled("\u{2190}\u{2192}", key_style),
            Span::styled(" fold", desc_style),
            sep.clone(),
            Span::styled("enter", key_style),
            Span::styled(" run", desc_style),
            sep.clone(),
            Span::styled("a", key_style),
            Span::styled(" all", desc_style),
            sep.clone(),
            Span::styled("c", key_style),
            Span::styled(" coverage", desc_style),
            sep.clone(),
            Span::styled("w", key_style),
            Span::styled(" watch", desc_style),
            sep.clone(),
            Span::styled("p", key_style),
            Span::styled(" parallel", desc_style),
            sep.clone(),
            Span::styled("f", key_style),
            Span::styled(" filter", desc_style),
            sep.clone(),
            Span::styled("q", key_style),
            Span::styled(" quit", desc_style),
        ],
        ViewMode::CoverageTable => vec![
            Span::styled("\u{2191}\u{2193}", key_style),
            Span::styled(" navigate", desc_style),
            sep.clone(),
            Span::styled("enter", key_style),
            Span::styled(" drill-in", desc_style),
            sep.clone(),
            Span::styled("s", key_style),
            Span::styled(" sort", desc_style),
            sep.clone(),
            Span::styled("t", key_style),
            Span::styled(" threshold", desc_style),
            sep.clone(),
            Span::styled("c", key_style),
            Span::styled(" back to tree", desc_style),
            sep.clone(),
            Span::styled("q", key_style),
            Span::styled(" quit", desc_style),
        ],
        ViewMode::CoverageSource => vec![
            Span::styled("\u{2191}\u{2193}", key_style),
            Span::styled(" scroll", desc_style),
            sep.clone(),
            Span::styled("n", key_style),
            Span::styled(" next uncovered", desc_style),
            sep.clone(),
            Span::styled("esc", key_style),
            Span::styled(" back to table", desc_style),
            sep.clone(),
            Span::styled("q", key_style),
            Span::styled(" quit", desc_style),
        ],
    };

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}
