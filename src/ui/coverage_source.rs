use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, LineCoverageStatus};

pub fn render_coverage_source(f: &mut Frame, area: Rect, app: &App) {
    let height = area.height as usize;
    if height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Header line from the selected coverage file
    let header_text = if let Some(cf) = app.coverage_files.get(app.coverage_selected) {
        format!("{} \u{2014} {:.0}%", cf.path, cf.percent)
    } else {
        "No file selected".to_string()
    };

    lines.push(Line::from(Span::styled(
        header_text,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));

    // Source lines starting from coverage_source_scroll
    let available_height = height.saturating_sub(1); // subtract header line
    let source_lines = &app.coverage_source_lines;

    for line_data in source_lines
        .iter()
        .skip(app.coverage_source_scroll)
        .take(available_height)
    {
        // Line number: right-aligned, 4 chars, DarkGray
        let line_num = format!("{:>4} ", line_data.line_number);

        // Coverage marker and background color based on status
        let (marker, marker_color, bg_color) = match line_data.status {
            LineCoverageStatus::Covered => (
                "\u{2588}\u{2588}",
                Color::Green,
                Color::Rgb(0, 40, 0),
            ),
            LineCoverageStatus::Uncovered => (
                "\u{2591}\u{2591}",
                Color::Red,
                Color::Rgb(40, 0, 0),
            ),
            LineCoverageStatus::NotExecutable => (
                "  ",
                Color::Reset,
                Color::Reset,
            ),
        };

        let spans = vec![
            Span::styled(
                line_num,
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                marker.to_string(),
                Style::default().fg(marker_color),
            ),
            Span::styled(
                format!(" {}", line_data.content),
                Style::default().bg(bg_color),
            ),
        ];

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}
