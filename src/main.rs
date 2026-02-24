mod app;
mod pest;
mod tree;
mod ui;
mod watcher;

use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
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

#[derive(Parser)]
#[command(name = "pesticide", version, about = "A TUI for running Pest PHP tests")]
struct Cli {
    /// Path to Laravel project root (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Disable parallel test execution
    #[arg(long)]
    no_parallel: bool,

    /// Start with watch mode enabled
    #[arg(short, long)]
    watch: bool,

    /// Run all tests immediately on launch
    #[arg(short, long)]
    run: bool,

    /// Run all tests with coverage immediately on launch
    #[arg(long)]
    coverage: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Find project root — use --path if provided, otherwise detect from cwd
    let base_dir = match cli.path {
        Some(ref p) => std::fs::canonicalize(p)?,
        None => std::env::current_dir()?,
    };
    let project_root = discovery::find_project_root(&base_dir).unwrap_or_else(|| base_dir.clone());

    // Discover tests
    let tree_root = match discovery::run_list_tests(&project_root) {
        Ok(output) => discovery::parse_test_list(&output, &project_root),
        Err(_) => tree::node::TreeNode::new_root(project_root.join("tests")),
    };

    let mut app = App::new(tree_root, project_root);
    app.parallel = !cli.no_parallel;
    app.watching = cli.watch;

    // Determine initial run mode from CLI flags
    let initial_coverage = cli.coverage;
    let initial_run = cli.run || cli.coverage;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app, initial_run, initial_coverage);

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

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    initial_run: bool,
    initial_coverage: bool,
) -> Result<()> {
    let mut run_handle: Option<pest::runner::RunHandle> = None;

    // If --run or --coverage was passed, kick off the test run immediately
    if initial_run {
        start_test_run(app, RunScope::All, &mut run_handle, initial_coverage);
    }

    // Channel for file-watcher events
    let (watch_tx, watch_rx) = mpsc::channel::<WatchEvent>();

    // Holds the debouncer while watch mode is active; dropping it stops the watcher.
    let mut debouncer: Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> = None;

    // Track the previous watching state so we can detect toggles.
    let mut was_watching = false;

    loop {
        app.tick = app.tick.wrapping_add(1);
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c')
                {
                    app.should_quit = true;
                }

                if app.filter_active {
                    handle_filter_keys(app, key.code);
                } else {
                    match app.view_mode {
                        ViewMode::Tree => handle_tree_keys(app, key.code, &mut run_handle),
                        ViewMode::CoverageTable => handle_coverage_table_keys(app, key.code),
                        ViewMode::CoverageTree => handle_coverage_tree_keys(app, key.code),
                        ViewMode::CoverageSource => handle_coverage_source_keys(app, key),
                    }
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
                        start_test_run(app, RunScope::File(path), &mut run_handle, false);
                    }
                    WatchEvent::SourceFileChanged(_path) => {
                        start_test_run(app, RunScope::All, &mut run_handle, false);
                    }
                    WatchEvent::TestFileCreatedOrDeleted => {
                        // Re-discover tests, then run all
                        if let Ok(output) = discovery::run_list_tests(&app.project_root) {
                            app.tree = discovery::parse_test_list(&output, &app.project_root);
                        }
                        start_test_run(app, RunScope::All, &mut run_handle, false);
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

                    // Parse JUnit results and update tree
                    let results = pest::runner::parse_junit_results(&app.project_root);
                    let total = results.len();
                    let mut matched = 0;
                    for result in &results {
                        if app.apply_test_result(result) {
                            matched += 1;
                        }
                    }
                    if total > 0 {
                        app.status_message = format!(
                            "Done: {}/{} tests matched",
                            matched, total
                        );
                    } else {
                        app.status_message = "Done: no results found in JUnit XML".to_string();
                    }

                    // If a coverage run just finished, load the results and switch view
                    if app.coverage_pending {
                        app.coverage_pending = false;
                        match app.load_coverage() {
                            Ok(()) => {
                                app.build_coverage_tree();
                                app.status_message = format!(
                                    "Coverage loaded: {} files",
                                    app.coverage_files.len()
                                );
                                app.view_mode = ViewMode::CoverageTable;
                            }
                            Err(e) => {
                                app.status_message = format!("Coverage error: {}", e);
                            }
                        }
                    }
                }
                Ok(None) => {
                    // Still running
                }
                Err(_) => {
                    app.running = false;
                    app.coverage_pending = false;
                    run_handle = None;
                }
            }
        }

        // Handle coverage drill-in (Enter pressed on coverage table)
        if app.coverage_drill_pending {
            app.coverage_drill_pending = false;
            match app.load_coverage_source() {
                Ok(()) => {
                    app.view_mode = ViewMode::CoverageSource;
                    // Jump to first uncovered line if one exists
                    if let Some(pos) = app
                        .coverage_source_lines
                        .iter()
                        .position(|l| l.status == crate::app::LineCoverageStatus::Uncovered)
                    {
                        app.coverage_source_scroll = pos;
                    }
                }
                Err(e) => {
                    app.status_message = format!("Coverage source error: {}", e);
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
    coverage: bool,
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

    // Track whether coverage was requested so we can load results when the run completes
    app.coverage_pending = coverage;

    // Spawn the test run
    match pest::runner::run_tests(
        &app.project_root,
        &scope,
        app.parallel,
        coverage,
        app.shared_output.clone(),
        app.shared_results.clone(),
    ) {
        Ok(handle) => {
            *run_handle = Some(handle);
        }
        Err(e) => {
            app.running = false;
            app.coverage_pending = false;
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
        KeyCode::Char('c') => {
            app.status_message = "Running tests with coverage...".to_string();
            start_test_run(app, RunScope::All, run_handle, true);
        }
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
                start_test_run(app, scope, run_handle, false);
            }
        }
        KeyCode::Char('f') => {
            app.filter_active = true;
            app.filter_text = Some(String::new());
        }
        KeyCode::Char('a') => {
            start_test_run(app, RunScope::All, run_handle, false);
        }
        KeyCode::Esc => {
            // Clear any active filter
            if app.filter_text.is_some() {
                app.filter_text = None;
                app.selected_index = 0;
            }
        }
        _ => {}
    }
}

fn handle_filter_keys(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char(c) => {
            if let Some(ref mut text) = app.filter_text {
                text.push(c);
            }
            app.selected_index = 0;
        }
        KeyCode::Backspace => {
            if let Some(ref mut text) = app.filter_text {
                text.pop();
            }
            app.selected_index = 0;
        }
        KeyCode::Enter => {
            // Confirm filter: keep filter_text, exit filter input mode
            app.filter_active = false;
        }
        KeyCode::Esc => {
            // Cancel filter: clear filter_text and exit filter input mode
            app.filter_active = false;
            app.filter_text = None;
            app.selected_index = 0;
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
        KeyCode::Char('t') => {
            app.build_coverage_tree();
            app.view_mode = ViewMode::CoverageTree;
        }
        KeyCode::Enter => {
            app.coverage_drill_pending = true;
        }
        _ => {}
    }
}

fn handle_coverage_tree_keys(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => app.view_mode = ViewMode::Tree,
        KeyCode::Char('t') => app.view_mode = ViewMode::CoverageTable,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.coverage_tree_selected > 0 {
                app.coverage_tree_selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.visible_coverage_tree_nodes().len().saturating_sub(1);
            if app.coverage_tree_selected < max {
                app.coverage_tree_selected += 1;
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.toggle_coverage_tree_expand(); // collapse
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.toggle_coverage_tree_expand(); // expand
        }
        KeyCode::Enter => {
            // Drill into file source view if a file is selected
            if let Some(file) = app.selected_coverage_tree_file() {
                // Find the index of this file in coverage_files for load_coverage_source
                if let Some(idx) = app
                    .coverage_files
                    .iter()
                    .position(|f| f.path == file.path)
                {
                    app.coverage_selected = idx;
                    app.coverage_drill_pending = true;
                }
            }
        }
        _ => {}
    }
}

fn handle_coverage_source_keys(app: &mut App, key: event::KeyEvent) {
    let half_page = 20;

    // Handle Ctrl+U / Ctrl+D for half-page scrolling
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('u') => {
                app.coverage_source_scroll = app.coverage_source_scroll.saturating_sub(half_page);
                return;
            }
            KeyCode::Char('d') => {
                let max = app.coverage_source_lines.len().saturating_sub(1);
                app.coverage_source_scroll = (app.coverage_source_scroll + half_page).min(max);
                return;
            }
            _ => {}
        }
    }

    match key.code {
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
