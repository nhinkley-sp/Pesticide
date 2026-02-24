use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Text,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, FocusPanel};

pub fn render_output(f: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == FocusPanel::Output {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color))
        .title(" Output ");

    let visible_lines: Vec<&str> = app
        .output_lines
        .iter()
        .skip(app.output_scroll)
        .map(|s| s.as_str())
        .collect();

    let text = Text::from(visible_lines.join("\n"));
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}
