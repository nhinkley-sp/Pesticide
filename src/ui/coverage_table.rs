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

    // Fixed column widths: " Lines" (7) + " Hit" (7) + " Miss" (7) + " pct%" (6) = 27
    let fixed_cols: usize = 27;
    let total_width = area.width as usize;
    // Path column takes all remaining width (minimum 20 chars)
    let path_width = total_width.saturating_sub(fixed_cols).max(20);

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

    // Column headers
    let header_text = format!(
        "{:<pw$} {:>6} {:>6} {:>6} {:>5}",
        "File", "Lines", "Hit", "Miss", "%",
        pw = path_width,
    );
    lines.push(Line::from(Span::styled(
        header_text,
        Style::default().add_modifier(Modifier::UNDERLINED),
    )));

    // File rows
    for (i, cf) in app.coverage_files.iter().enumerate() {
        let is_selected = i == app.coverage_selected;
        let below_threshold = cf.percent < app.coverage_threshold;

        // Truncate file path with ellipsis only if it exceeds available width
        let display_path = if cf.path.len() > path_width {
            let truncated = &cf.path[cf.path.len() - (path_width - 1)..];
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

        // Split path into directory + filename, highlight the filename
        let (dir_part, file_part) = match display_path.rfind('/') {
            Some(pos) => (&display_path[..=pos], &display_path[pos + 1..]),
            None => ("", display_path.as_str()),
        };
        let padding_needed = path_width.saturating_sub(display_path.len());

        let mut spans = vec![
            Span::styled(
                dir_part.to_string(),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                file_part.to_string(),
                Style::default().fg(text_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ".repeat(padding_needed)),
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
