use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::tree::node::{NodeKind, TestStatus};

pub fn render_tree(f: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible_nodes();
    let height = area.height as usize;

    if visible.is_empty() || height == 0 {
        let paragraph = Paragraph::new("No tests found.");
        f.render_widget(paragraph, area);
        return;
    }

    // Calculate scroll offset to keep selected item visible
    let scroll_offset = compute_scroll_offset(app.selected_index, height, visible.len());

    let area_width = area.width as usize;

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(height)
        .map(|(i, (depth, node))| {
            let is_selected = i == app.selected_index;

            // Indentation: 2 spaces per depth level
            let indent = "  ".repeat(*depth);

            // Status icon
            let status = if node.children.is_empty() {
                node.status.clone()
            } else {
                node.aggregate_status()
            };
            let (status_icon, status_color) = match status {
                TestStatus::Passed => ("\u{2713}", Color::Green),
                TestStatus::Failed => ("\u{2717}", Color::Red),
                TestStatus::Running => ("\u{27f3}", Color::Yellow),
                TestStatus::NotRun => ("\u{25cc}", Color::DarkGray),
            };

            // Expand/collapse icon
            let expand_icon = match node.kind {
                NodeKind::Root | NodeKind::Directory | NodeKind::File => {
                    if node.expanded {
                        "\u{25bc} "
                    } else {
                        "\u{25b6} "
                    }
                }
                NodeKind::Test => "  ",
            };

            // Test count suffix for non-test nodes
            let count_suffix = match node.kind {
                NodeKind::Test => String::new(),
                _ => {
                    if node.test_count > 0 {
                        format!(" ({})", node.test_count)
                    } else {
                        String::new()
                    }
                }
            };

            // Coverage percentage (right-aligned)
            let coverage_text = node.coverage_percent.map(|pct| {
                let color = if pct >= 80.0 {
                    Color::Green
                } else if pct >= 50.0 {
                    Color::Yellow
                } else {
                    Color::Red
                };
                (format!("{:.0}%", pct), color)
            });

            // Build spans
            let mut spans = Vec::new();

            // Indent span
            spans.push(Span::raw(indent.clone()));

            // Status icon span
            spans.push(Span::styled(
                status_icon.to_string(),
                Style::default().fg(status_color),
            ));

            // Space + expand icon
            spans.push(Span::raw(format!(" {}", expand_icon)));

            // Node name
            let name_style = if is_selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            spans.push(Span::styled(node.name.clone(), name_style));

            // Count suffix
            if !count_suffix.is_empty() {
                spans.push(Span::styled(
                    count_suffix.clone(),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            // Coverage percentage right-aligned
            if let Some((cov_text, cov_color)) = &coverage_text {
                // Calculate padding to right-align
                let left_len = indent.len() + 1 + 1 + expand_icon.len() + node.name.len() + count_suffix.len();
                let cov_len = cov_text.len();
                let padding = if area_width > left_len + cov_len + 1 {
                    area_width - left_len - cov_len
                } else {
                    1
                };
                spans.push(Span::raw(" ".repeat(padding)));
                spans.push(Span::styled(
                    cov_text.clone(),
                    Style::default().fg(*cov_color),
                ));
            }

            let mut line = Line::from(spans);

            // Apply selected row background
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

/// Computes the scroll offset needed to keep the selected index visible.
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
