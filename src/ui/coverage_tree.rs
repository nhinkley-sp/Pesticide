use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;

pub fn render_coverage_tree(f: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible_coverage_tree_nodes();
    let height = area.height as usize;

    if visible.is_empty() || height == 0 {
        let paragraph = Paragraph::new("No coverage data. Press t to switch to table view.");
        f.render_widget(paragraph, area);
        return;
    }

    let scroll_offset = compute_scroll_offset(app.coverage_tree_selected, height, visible.len());
    let area_width = area.width as usize;

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(height)
        .map(|(i, (depth, node))| {
            let is_selected = i == app.coverage_tree_selected;

            // Indentation: 2 spaces per depth level
            let indent = "  ".repeat(*depth);

            // Expand/collapse icon for directories, blank for files
            let expand_icon = if node.is_file {
                "  "
            } else if node.expanded {
                "\u{25bc} "
            } else {
                "\u{25b6} "
            };

            // Coverage percentage color
            let pct_color = if node.percent >= 80.0 {
                Color::Green
            } else if node.percent >= 50.0 {
                Color::Yellow
            } else {
                Color::Red
            };

            // Build the right-side stats text
            let stats_text = if node.is_file {
                format!("{:>4.0}%  {:>5} lines", node.percent, node.lines)
            } else {
                format!(
                    "{:>4.0}%  {:>5} lines, {:>4} uncovered",
                    node.percent, node.lines, node.misses
                )
            };

            let stats_len = stats_text.len();

            // Calculate name portion length for padding
            let name_portion_len = indent.len() + expand_icon.len() + node.name.len();
            let padding = if area_width > name_portion_len + stats_len + 2 {
                area_width - name_portion_len - stats_len
            } else {
                2
            };

            let name_style = if is_selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else if node.is_file {
                Style::default()
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };

            let mut spans = vec![
                Span::raw(indent),
                Span::styled(expand_icon.to_string(), Style::default().fg(Color::DarkGray)),
                Span::styled(node.name.clone(), name_style),
                Span::raw(" ".repeat(padding)),
                Span::styled(stats_text, Style::default().fg(pct_color)),
            ];

            if is_selected {
                spans = spans
                    .into_iter()
                    .map(|s| {
                        Span::styled(
                            s.content.to_string(),
                            s.style.bg(Color::DarkGray),
                        )
                    })
                    .collect();
            }

            let mut line = Line::from(spans);
            if is_selected {
                line = line.style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
            }

            line
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn compute_scroll_offset(selected: usize, height: usize, total: usize) -> usize {
    if total <= height {
        return 0;
    }
    if selected < height / 3 {
        return 0;
    }
    let ideal = selected.saturating_sub(height / 3);
    let max_offset = total.saturating_sub(height);
    ideal.min(max_offset)
}
