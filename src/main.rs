mod app;
mod pest;
mod tree;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use app::{App, FocusPanel, ViewMode};
use pest::discovery;

fn main() -> Result<()> {
    // Find project root
    let cwd = std::env::current_dir()?;
    let project_root = discovery::find_project_root(&cwd).unwrap_or_else(|| cwd.clone());

    // Discover tests
    let tree_root = match discovery::run_list_tests(&project_root) {
        Ok(output) => discovery::parse_test_list(&output, &project_root),
        Err(_) => tree::node::TreeNode::new_root(project_root.join("tests")),
    };

    let mut app = App::new(tree_root, project_root);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c')
                {
                    app.should_quit = true;
                }

                match app.view_mode {
                    ViewMode::Tree => handle_tree_keys(app, key.code),
                    ViewMode::CoverageTable => handle_coverage_table_keys(app, key.code),
                    ViewMode::CoverageSource => handle_coverage_source_keys(app, key.code),
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_tree_keys(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.focus == FocusPanel::Tree {
                app.move_up();
            } else {
                app.output_scroll = app.output_scroll.saturating_sub(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.focus == FocusPanel::Tree {
                app.move_down();
            } else {
                let max = app.output_lines.len().saturating_sub(1);
                if app.output_scroll < max {
                    app.output_scroll += 1;
                }
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if app.focus == FocusPanel::Tree {
                app.toggle_expand();
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if app.focus == FocusPanel::Tree {
                app.toggle_expand();
            }
        }
        KeyCode::Char('p') => app.toggle_parallel(),
        KeyCode::Char('w') => app.toggle_watch(),
        KeyCode::Char('c') => app.view_mode = ViewMode::CoverageTable,
        KeyCode::Tab => {
            app.focus = match app.focus {
                FocusPanel::Tree => FocusPanel::Output,
                FocusPanel::Output => FocusPanel::Tree,
            };
        }
        KeyCode::Char('G') => {
            if app.focus == FocusPanel::Output {
                app.output_scroll = app.output_lines.len().saturating_sub(1);
            }
        }
        KeyCode::Char('g') => {
            if app.focus == FocusPanel::Output {
                app.output_scroll = 0;
            }
        }
        KeyCode::Enter => {
            // Test running will be wired in Task 9
        }
        KeyCode::Char('a') => {
            // Run all will be wired in Task 9
        }
        _ => {}
    }
}

fn handle_coverage_table_keys(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') | KeyCode::Esc => app.view_mode = ViewMode::Tree,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.coverage_selected > 0 {
                app.coverage_selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.coverage_files.len().saturating_sub(1);
            if app.coverage_selected < max {
                app.coverage_selected += 1;
            }
        }
        KeyCode::Char('s') => app.cycle_coverage_sort(),
        KeyCode::Enter => app.view_mode = ViewMode::CoverageSource,
        _ => {}
    }
}

fn handle_coverage_source_keys(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => app.view_mode = ViewMode::CoverageTable,
        KeyCode::Up | KeyCode::Char('k') => {
            app.coverage_source_scroll = app.coverage_source_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.coverage_source_lines.len().saturating_sub(1);
            if app.coverage_source_scroll < max {
                app.coverage_source_scroll += 1;
            }
        }
        KeyCode::Char('n') => {
            // Jump to next uncovered line
            if let Some(pos) = app
                .coverage_source_lines
                .iter()
                .skip(app.coverage_source_scroll + 1)
                .position(|l| l.status == crate::app::LineCoverageStatus::Uncovered)
            {
                app.coverage_source_scroll += pos + 1;
            }
        }
        _ => {}
    }
}
