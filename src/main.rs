mod app;
mod pest;
mod tree;
mod ui;
mod watcher;

use std::io;
use std::sync::mpsc;
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
use pest::runner::RunScope;
use tree::node::NodeKind;
use watcher::WatchEvent;

#[tokio::main]
async fn main() -> Result<()> {
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
    let mut run_handle: Option<pest::runner::RunHandle> = None;

    // Channel for file-watcher events
    let (watch_tx, watch_rx) = mpsc::channel::<WatchEvent>();

    // Holds the debouncer while watch mode is active; dropping it stops the watcher.
    let mut debouncer: Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> = None;

    // Track the previous watching state so we can detect toggles.
    let mut was_watching = false;

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
                    ViewMode::Tree => handle_tree_keys(app, key.code, &mut run_handle),
                    ViewMode::CoverageTable => handle_coverage_table_keys(app, key.code),
                    ViewMode::CoverageSource => handle_coverage_source_keys(app, key.code),
                }
            }
        }

        // Handle watch mode toggling
        if app.watching && !was_watching {
            // Watch mode just turned ON: start the watcher.
            match watcher::start_watcher(&app.project_root, watch_tx.clone()) {
                Ok(d) => {
                    debouncer = Some(d);
                    app.status_message = "Watch mode ON".to_string();
                }
                Err(e) => {
                    app.watching = false;
                    app.status_message = format!("Watch error: {}", e);
                }
            }
        } else if !app.watching && was_watching {
            // Watch mode just turned OFF: drop the debouncer to stop watching.
            debouncer = None;
            app.status_message = "Watch mode OFF".to_string();
        }
        was_watching = app.watching;

        // Process file-watcher events (non-blocking)
        if app.watching {
            while let Ok(watch_event) = watch_rx.try_recv() {
                match watch_event {
                    WatchEvent::TestFileChanged(path) => {
                        start_test_run(app, RunScope::File(path), &mut run_handle);
                    }
                    WatchEvent::SourceFileChanged(_path) => {
                        start_test_run(app, RunScope::All, &mut run_handle);
                    }
                    WatchEvent::TestFileCreatedOrDeleted => {
                        // Re-discover tests, then run all
                        if let Ok(output) = discovery::run_list_tests(&app.project_root) {
                            app.tree = discovery::parse_test_list(&output, &app.project_root);
                        }
                        start_test_run(app, RunScope::All, &mut run_handle);
                    }
                }
            }
        }

        // Sync shared output from runner into app state
        app.sync_output();

        // Check if child process has exited
        if let Some(ref mut handle) = run_handle {
            match handle.child.try_wait() {
                Ok(Some(_status)) => {
                    app.running = false;
                    run_handle = None;
                }
                Ok(None) => {
                    // Still running
                }
                Err(_) => {
                    app.running = false;
                    run_handle = None;
                }
            }
        }

        if app.should_quit {
            // Kill running process if any
            if let Some(ref mut handle) = run_handle {
                handle.kill();
            }
            // Drop the debouncer to stop the watcher
            drop(debouncer);
            return Ok(());
        }
    }
}

fn start_test_run(
    app: &mut App,
    scope: RunScope,
    run_handle: &mut Option<pest::runner::RunHandle>,
) {
    // Clear output state
    app.output_lines.clear();
    app.output_scroll = 0;
    if let Ok(mut lines) = app.shared_output.lock() {
        lines.clear();
    }
    if let Ok(mut res) = app.shared_results.lock() {
        res.clear();
    }

    app.running = true;

    // Kill existing run if any
    if let Some(ref mut handle) = run_handle {
        handle.kill();
    }
    *run_handle = None;

    // Spawn the test run
    match pest::runner::run_tests(
        &app.project_root,
        &scope,
        app.parallel,
        false,
        app.shared_output.clone(),
        app.shared_results.clone(),
    ) {
        Ok(handle) => {
            *run_handle = Some(handle);
        }
        Err(e) => {
            app.running = false;
            app.output_lines
                .push(format!("Failed to start tests: {}", e));
        }
    }
}

fn handle_tree_keys(
    app: &mut App,
    key: KeyCode,
    run_handle: &mut Option<pest::runner::RunHandle>,
) {
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
            // Determine scope based on selected node kind
            if let Some(node) = app.selected_node() {
                let scope = match &node.kind {
                    NodeKind::Root => RunScope::All,
                    NodeKind::Directory => RunScope::Directory(node.path.clone()),
                    NodeKind::File => RunScope::File(node.path.clone()),
                    NodeKind::Test => RunScope::Test {
                        file: node.path.clone(),
                        name: node.name.clone(),
                    },
                };
                start_test_run(app, scope, run_handle);
            }
        }
        KeyCode::Char('a') => {
            start_test_run(app, RunScope::All, run_handle);
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
