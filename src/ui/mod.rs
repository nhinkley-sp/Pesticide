mod coverage_source;
mod coverage_table;
mod coverage_tree;
mod footer;
mod output;
mod tree;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, ViewMode};

/// Main render function that draws the entire TUI.
///
/// Layout (vertical):
///   1. Header  — 1 line
///   2. Main    — flexible (tree view, coverage table, or coverage source)
///   3. Output  — 10 lines
///   4. Footer  — 1 line
pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Header
            Constraint::Min(5),    // Main content
            Constraint::Length(10), // Output panel
            Constraint::Length(1),  // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], app);

    match app.view_mode {
        ViewMode::Tree => tree::render_tree(f, chunks[1], app),
        ViewMode::CoverageTable => coverage_table::render_coverage_table(f, chunks[1], app),
        ViewMode::CoverageTree => coverage_tree::render_coverage_tree(f, chunks[1], app),
        ViewMode::CoverageSource => coverage_source::render_coverage_source(f, chunks[1], app),
    }

    output::render_output(f, chunks[2], app);
    footer::render_footer(f, chunks[3], app);
}

/// Renders the header line with project name, mode indicators, and status.
fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let project_name = app
        .project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.project_root.display().to_string());

    let mode_indicator = if app.parallel {
        "\u{2225} parallel"
    } else {
        "\u{2192} sequential"
    };

    let mut spans = vec![
        Span::styled(
            "Pesticide",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" \u{2014} {} \u{2014} {}", project_name, mode_indicator),
            Style::default().fg(Color::White),
        ),
    ];

    if app.watching {
        spans.push(Span::styled(
            "  \u{25c9} watch",
            Style::default().fg(Color::Magenta),
        ));
    }

    if app.running {
        const THROBBER: &[char] = &['\u{2801}', '\u{2809}', '\u{2819}', '\u{281b}', '\u{281e}', '\u{2836}', '\u{2834}', '\u{2824}', '\u{2826}', '\u{2827}'];
        let frame = THROBBER[app.tick % THROBBER.len()];
        spans.push(Span::styled(
            format!("  {} running", frame),
            Style::default().fg(Color::Yellow),
        ));
    }

    if let Some(ref filter) = app.filter_text {
        if !filter.is_empty() && !app.filter_active {
            spans.push(Span::styled(
                format!("  [filter: {}]", filter),
                Style::default().fg(Color::Yellow),
            ));
        }
    }

    if !app.status_message.is_empty() {
        spans.push(Span::styled(
            format!("  {}", app.status_message),
            Style::default().fg(Color::Cyan),
        ));
    }

    let line = Line::from(spans);
    let header = Paragraph::new(line);
    f.render_widget(header, area);
}
