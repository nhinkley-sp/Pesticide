use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, CoverageSort};

pub fn render_coverage_table(f: &mut Frame, area: Rect, app: &App) {
    let height = area.height as usize;
    if height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Calculate total coverage percentage from all files
    let total_lines: usize = app.coverage_files.iter().map(|f| f.lines).sum();
    let total_hits: usize = app.coverage_files.iter().map(|f| f.hits).sum();
    let total_pct = if total_lines > 0 {
        (total_hits as f64 / total_lines as f64) * 100.0
    } else {
        0.0
    };

    // Sort label based on current sort mode
    let sort_label = match app.coverage_sort {
        CoverageSort::PercentAsc => "% \u{2191}",
        CoverageSort::PercentDesc => "% \u{2193}",
        CoverageSort::MissesDesc => "Miss \u{2193}",
        CoverageSort::FileName => "Name",
    };

    // Header line
    lines.push(Line::from(Span::styled(
        format!("Coverage \u{2014} {:.0}% total \u{2014} sorted by: {}", total_pct, sort_label),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));

    // Blank line
    lines.push(Line::from(""));

    // Column headers: File  Lines  Hit  Miss  %
    // Use fixed-width columns for alignment
    let header_text = format!(
        "{:<45} {:>6} {:>6} {:>6} {:>5}",
        "File", "Lines", "Hit", "Miss", "%"
    );
    lines.push(Line::from(Span::styled(
        header_text,
        Style::default().add_modifier(Modifier::UNDERLINED),
    )));

    // File rows
    for (i, cf) in app.coverage_files.iter().enumerate() {
        let is_selected = i == app.coverage_selected;
        let below_threshold = cf.percent < app.coverage_threshold;

        // Truncate file path to 45 chars with "..." prefix if too long
        let display_path = if cf.path.len() > 45 {
            let truncated = &cf.path[cf.path.len() - 44..];
            format!("\u{2026}{}", truncated)
        } else {
            cf.path.clone()
        };

        // Percentage color coding
        let pct_color = if cf.percent >= 80.0 {
            Color::Green
        } else if cf.percent >= 50.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        // Base text color: Red if below threshold, otherwise default
        let text_color = if below_threshold {
            Color::Red
        } else {
            Color::Reset
        };

        // Row style for selected
        let row_style = if is_selected {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let mut spans = vec![
            Span::styled(
                format!("{:<45}", display_path),
                Style::default().fg(text_color),
            ),
            Span::styled(
                format!(" {:>6}", cf.lines),
                Style::default().fg(text_color),
            ),
            Span::styled(
                format!(" {:>6}", cf.hits),
                Style::default().fg(text_color),
            ),
            Span::styled(
                format!(" {:>6}", cf.misses),
                Style::default().fg(text_color),
            ),
            Span::styled(
                format!(" {:>4.0}%", cf.percent),
                Style::default().fg(pct_color),
            ),
        ];

        // Apply selected style to all spans
        if is_selected {
            spans = spans
                .into_iter()
                .map(|s| {
                    Span::styled(
                        s.content.to_string(),
                        s.style.bg(Color::DarkGray).add_modifier(Modifier::BOLD),
                    )
                })
                .collect();
        }

        let mut line = Line::from(spans);
        line = line.style(row_style);
        lines.push(line);
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}
