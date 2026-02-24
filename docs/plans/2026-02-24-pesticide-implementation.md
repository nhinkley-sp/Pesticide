# Pesticide Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust TUI for running Pest PHP tests in Laravel projects with tree view, coverage drill-down, watch mode, and parallel execution by default.

**Architecture:** Ratatui-based single-pane TUI. Shells out to `./vendor/bin/pest` for discovery and running. Parses TeamCity-format output for structured results, Clover XML for coverage. Async event loop via tokio.

**Tech Stack:** Rust, ratatui, crossterm, tokio, notify, roxmltree, clap

---

### Task 1: Install Rust Toolchain

**Files:** None (system setup)

**Step 1: Install rustup and stable toolchain**

Run:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

**Step 2: Verify installation**

Run:
```bash
source ~/.cargo/env && rustc --version && cargo --version
```
Expected: Version numbers printed for both.

---

### Task 2: Scaffold Project

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

**Step 1: Initialize cargo project**

Run:
```bash
cd /Users/nhinkley/Repos/Pesticide
cargo init --name pesticide
```

**Step 2: Set up Cargo.toml with dependencies**

Replace `Cargo.toml` with:

```toml
[package]
name = "pesticide"
version = "0.1.0"
edition = "2021"
description = "A TUI for running Pest PHP tests in Laravel projects"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
tokio = { version = "1", features = ["full"] }
notify = "7"
notify-debouncer-mini = "0.5"
roxmltree = "0.20"
clap = { version = "4", features = ["derive"] }
anyhow = "1"

[dev-dependencies]
tempfile = "3"
```

**Step 3: Write minimal main.rs**

```rust
fn main() {
    println!("pesticide v0.1.0");
}
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

**Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: scaffold Pesticide project with dependencies"
```

---

### Task 3: Tree Data Structure

**Files:**
- Create: `src/tree/mod.rs`
- Create: `src/tree/node.rs`
- Modify: `src/main.rs` (add `mod tree;`)

**Step 1: Write tests for tree node**

In `src/tree/node.rs`:

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum TestStatus {
    NotRun,
    Running,
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Root,
    Directory,
    File,
    Test,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub kind: NodeKind,
    pub path: PathBuf,
    pub status: TestStatus,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
    pub coverage_percent: Option<f64>,
    pub test_count: usize,
}

impl TreeNode {
    pub fn new_directory(name: &str, path: PathBuf) -> Self {
        Self {
            name: name.to_string(),
            kind: NodeKind::Directory,
            path,
            status: TestStatus::NotRun,
            expanded: false,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 0,
        }
    }

    pub fn new_file(name: &str, path: PathBuf) -> Self {
        Self {
            name: name.to_string(),
            kind: NodeKind::File,
            path,
            status: TestStatus::NotRun,
            expanded: false,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 0,
        }
    }

    pub fn new_test(name: &str, path: PathBuf) -> Self {
        Self {
            name: name.to_string(),
            kind: NodeKind::Test,
            path,
            status: TestStatus::NotRun,
            expanded: false,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 1,
        }
    }

    pub fn new_root(path: PathBuf) -> Self {
        Self {
            name: "tests/".to_string(),
            kind: NodeKind::Root,
            path,
            status: TestStatus::NotRun,
            expanded: true,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 0,
        }
    }

    pub fn add_child(&mut self, child: TreeNode) {
        self.test_count += child.test_count;
        self.children.push(child);
    }

    /// Returns a flat list of (depth, &TreeNode) for visible (expanded) nodes.
    pub fn flatten(&self) -> Vec<(usize, &TreeNode)> {
        let mut result = Vec::new();
        self.flatten_recursive(0, &mut result);
        result
    }

    fn flatten_recursive<'a>(&'a self, depth: usize, result: &mut Vec<(usize, &'a TreeNode)>) {
        result.push((depth, self));
        if self.expanded {
            for child in &self.children {
                child.flatten_recursive(depth + 1, result);
            }
        }
    }

    /// Recalculate test counts from children.
    pub fn recalculate_counts(&mut self) {
        if self.kind == NodeKind::Test {
            self.test_count = 1;
            return;
        }
        self.test_count = 0;
        for child in &mut self.children {
            child.recalculate_counts();
            self.test_count += child.test_count;
        }
    }

    /// Aggregate status: Failed if any child failed, Passed if all passed, Running if any running.
    pub fn aggregate_status(&self) -> TestStatus {
        if self.kind == NodeKind::Test {
            return self.status.clone();
        }
        let mut any_failed = false;
        let mut any_running = false;
        let mut all_passed = true;
        let mut any_run = false;

        for child in &self.children {
            let child_status = child.aggregate_status();
            match child_status {
                TestStatus::Failed => {
                    any_failed = true;
                    any_run = true;
                }
                TestStatus::Running => {
                    any_running = true;
                    any_run = true;
                }
                TestStatus::Passed => {
                    any_run = true;
                }
                TestStatus::NotRun => {
                    all_passed = false;
                }
            }
        }

        if any_failed {
            TestStatus::Failed
        } else if any_running {
            TestStatus::Running
        } else if any_run && all_passed {
            TestStatus::Passed
        } else {
            TestStatus::NotRun
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_directory() {
        let node = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        assert_eq!(node.name, "Feature");
        assert_eq!(node.kind, NodeKind::Directory);
        assert!(!node.expanded);
        assert_eq!(node.test_count, 0);
    }

    #[test]
    fn test_add_child_updates_count() {
        let mut dir = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        let test = TreeNode::new_test("it works", PathBuf::from("tests/Feature/ExampleTest.php"));
        dir.add_child(test);
        assert_eq!(dir.test_count, 1);
    }

    #[test]
    fn test_flatten_collapsed() {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut dir = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        dir.add_child(TreeNode::new_test("it works", PathBuf::from("tests/Feature/ExampleTest.php")));
        root.add_child(dir);
        // root is expanded, Feature is collapsed
        let flat = root.flatten();
        assert_eq!(flat.len(), 2); // root + Feature (collapsed, hides child)
    }

    #[test]
    fn test_flatten_expanded() {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut dir = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        dir.expanded = true;
        dir.add_child(TreeNode::new_test("it works", PathBuf::from("tests/Feature/ExampleTest.php")));
        root.add_child(dir);
        let flat = root.flatten();
        assert_eq!(flat.len(), 3); // root + Feature + test
    }

    #[test]
    fn test_aggregate_status_all_passed() {
        let mut dir = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        let mut t1 = TreeNode::new_test("t1", PathBuf::from("t1"));
        t1.status = TestStatus::Passed;
        let mut t2 = TreeNode::new_test("t2", PathBuf::from("t2"));
        t2.status = TestStatus::Passed;
        dir.add_child(t1);
        dir.add_child(t2);
        assert_eq!(dir.aggregate_status(), TestStatus::Passed);
    }

    #[test]
    fn test_aggregate_status_any_failed() {
        let mut dir = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        let mut t1 = TreeNode::new_test("t1", PathBuf::from("t1"));
        t1.status = TestStatus::Passed;
        let mut t2 = TreeNode::new_test("t2", PathBuf::from("t2"));
        t2.status = TestStatus::Failed;
        dir.add_child(t1);
        dir.add_child(t2);
        assert_eq!(dir.aggregate_status(), TestStatus::Failed);
    }

    #[test]
    fn test_aggregate_status_not_run() {
        let dir = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        assert_eq!(dir.aggregate_status(), TestStatus::NotRun);
    }
}
```

**Step 2: Create mod.rs**

In `src/tree/mod.rs`:
```rust
pub mod node;
```

**Step 3: Wire up module in main.rs**

```rust
mod tree;

fn main() {
    println!("pesticide v0.1.0");
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: All 7 tests pass.

**Step 5: Commit**

```bash
git add src/tree/ src/main.rs
git commit -m "feat: add tree data structure with node types and status aggregation"
```

---

### Task 4: Pest Discovery — Parse `--list-tests` Output

**Files:**
- Create: `src/pest/mod.rs`
- Create: `src/pest/discovery.rs`
- Modify: `src/main.rs` (add `mod pest;`)

**Context:** `pest --list-tests` outputs lines like:
```
- Tests\Feature\Auth\LoginTest::it_can_login_with_valid_credentials
- Tests\Feature\Auth\LoginTest::it_rejects_invalid_password
- Tests\Unit\Models\PlayerTest::it_returns_false_when_player_has_no_injuries
```

Each line is `- Namespace\Class::test_name`. We need to parse this into the tree structure: directories → files → tests.

**Step 1: Write tests for parsing**

In `src/pest/discovery.rs`:

```rust
use std::path::{Path, PathBuf};
use anyhow::Result;
use crate::tree::node::TreeNode;

/// Parse the output of `pest --list-tests` into a tree.
pub fn parse_test_list(output: &str, project_root: &Path) -> TreeNode {
    let mut root = TreeNode::new_root(project_root.join("tests"));

    for line in output.lines() {
        let line = line.trim();
        // Lines look like: "- Tests\Feature\Auth\LoginTest::it_can_login_with_valid_credentials"
        let line = match line.strip_prefix("- ") {
            Some(l) => l,
            None => continue,
        };

        // Split on :: to get class and test name
        let (class_part, test_name) = match line.split_once("::") {
            Some((c, t)) => (c, t),
            None => continue,
        };

        // Convert namespace to path segments: Tests\Feature\Auth\LoginTest -> [Feature, Auth, LoginTest]
        let segments: Vec<&str> = class_part.split('\\').collect();
        // Skip "Tests" prefix (or "P\Tests", etc.)
        let segments = if let Some(pos) = segments.iter().position(|s| s.eq_ignore_ascii_case("tests")) {
            &segments[pos + 1..]
        } else {
            &segments[1..] // skip first segment as namespace root
        };

        if segments.is_empty() {
            continue;
        }

        // Last segment is the class name (file), rest are directories
        let dirs = &segments[..segments.len() - 1];
        let file_name = segments[segments.len() - 1];

        // Navigate/create directory nodes
        let mut current = &mut root;
        let mut current_path = project_root.join("tests");

        for dir in dirs {
            current_path = current_path.join(dir);
            let dir_str = dir.to_string();
            let idx = current.children.iter().position(|c| c.name == dir_str);
            if idx.is_none() {
                let new_dir = TreeNode::new_directory(dir, current_path.clone());
                current.children.push(new_dir);
            }
            let idx = current.children.iter().position(|c| c.name == dir_str).unwrap();
            current = &mut current.children[idx];
        }

        // Navigate/create file node
        let file_display = format!("{}.php", file_name);
        current_path = current_path.join(&file_display);
        let file_idx = current.children.iter().position(|c| c.name == file_display);
        if file_idx.is_none() {
            let new_file = TreeNode::new_file(&file_display, current_path.clone());
            current.children.push(new_file);
        }
        let file_idx = current.children.iter().position(|c| c.name == file_display).unwrap();
        let file_node = &mut current.children[file_idx];

        // Add test node
        let test_display = test_name.replace('_', " ");
        let test_node = TreeNode::new_test(&test_display, current_path.clone());
        file_node.add_child(test_node);
    }

    root.recalculate_counts();
    root
}

/// Find the project root by searching upward for vendor/bin/pest.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join("vendor/bin/pest").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Run pest --list-tests and return stdout.
pub fn run_list_tests(project_root: &Path) -> Result<String> {
    let output = std::process::Command::new(project_root.join("vendor/bin/pest"))
        .arg("--list-tests")
        .current_dir(project_root)
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = "\
- Tests\\Feature\\Auth\\LoginTest::it_can_login_with_valid_credentials
- Tests\\Feature\\Auth\\LoginTest::it_rejects_invalid_password
- Tests\\Feature\\Auth\\RegisterTest::it_can_register_new_user
- Tests\\Unit\\Models\\PlayerTest::it_returns_false_when_player_has_no_injuries
- Tests\\Unit\\Models\\PlayerTest::it_returns_true_when_player_has_active_injury
";

    #[test]
    fn test_parse_creates_root() {
        let tree = parse_test_list(SAMPLE_OUTPUT, Path::new("/project"));
        assert_eq!(tree.name, "tests/");
        assert_eq!(tree.children.len(), 2); // Feature, Unit
    }

    #[test]
    fn test_parse_directory_structure() {
        let tree = parse_test_list(SAMPLE_OUTPUT, Path::new("/project"));
        let feature = &tree.children[0];
        assert_eq!(feature.name, "Feature");
        let auth = &feature.children[0];
        assert_eq!(auth.name, "Auth");
        assert_eq!(auth.children.len(), 2); // LoginTest.php, RegisterTest.php
    }

    #[test]
    fn test_parse_test_names() {
        let tree = parse_test_list(SAMPLE_OUTPUT, Path::new("/project"));
        let login_test = &tree.children[0].children[0].children[0]; // Feature > Auth > LoginTest.php
        assert_eq!(login_test.name, "LoginTest.php");
        assert_eq!(login_test.children.len(), 2);
        assert_eq!(login_test.children[0].name, "it can login with valid credentials");
    }

    #[test]
    fn test_parse_counts() {
        let tree = parse_test_list(SAMPLE_OUTPUT, Path::new("/project"));
        assert_eq!(tree.test_count, 5);
        let feature = &tree.children[0];
        assert_eq!(feature.test_count, 3);
        let unit = &tree.children[1];
        assert_eq!(unit.test_count, 2);
    }

    #[test]
    fn test_parse_empty_output() {
        let tree = parse_test_list("", Path::new("/project"));
        assert_eq!(tree.test_count, 0);
        assert!(tree.children.is_empty());
    }

    #[test]
    fn test_parse_malformed_lines() {
        let output = "not a test line\n- BadLine\n";
        let tree = parse_test_list(output, Path::new("/project"));
        assert_eq!(tree.test_count, 0);
    }
}
```

**Step 2: Create mod.rs**

In `src/pest/mod.rs`:
```rust
pub mod discovery;
```

**Step 3: Wire up in main.rs**

```rust
mod pest;
mod tree;

fn main() {
    println!("pesticide v0.1.0");
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass (tree + discovery).

**Step 5: Commit**

```bash
git add src/pest/ src/main.rs
git commit -m "feat: add Pest test discovery via --list-tests parsing"
```

---

### Task 5: App State & Event Loop

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs`

**Step 1: Define app state and event types**

In `src/app.rs`:

```rust
use std::path::PathBuf;
use crate::tree::node::TreeNode;

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    Tree,
    CoverageTable,
    CoverageSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FocusPanel {
    Tree,
    Output,
}

pub struct App {
    pub tree: TreeNode,
    pub project_root: PathBuf,
    pub selected_index: usize,
    pub output_lines: Vec<String>,
    pub output_scroll: usize,
    pub view_mode: ViewMode,
    pub focus: FocusPanel,
    pub parallel: bool,
    pub watching: bool,
    pub running: bool,
    pub should_quit: bool,
    pub filter_text: Option<String>,
    pub coverage_files: Vec<CoverageFile>,
    pub coverage_selected: usize,
    pub coverage_sort: CoverageSort,
    pub coverage_source_lines: Vec<CoverageSourceLine>,
    pub coverage_source_scroll: usize,
    pub coverage_threshold: f64,
    pub status_message: String,
}

#[derive(Debug, Clone)]
pub struct CoverageFile {
    pub path: String,
    pub lines: usize,
    pub hits: usize,
    pub misses: usize,
    pub percent: f64,
}

#[derive(Debug, Clone)]
pub struct CoverageSourceLine {
    pub line_number: usize,
    pub content: String,
    pub status: LineCoverageStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineCoverageStatus {
    Covered,
    Uncovered,
    NotExecutable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoverageSort {
    PercentAsc,
    PercentDesc,
    MissesDesc,
    FileName,
}

impl App {
    pub fn new(tree: TreeNode, project_root: PathBuf) -> Self {
        Self {
            tree,
            project_root,
            selected_index: 0,
            output_lines: Vec::new(),
            output_scroll: 0,
            view_mode: ViewMode::Tree,
            focus: FocusPanel::Tree,
            parallel: true,
            watching: false,
            running: false,
            should_quit: false,
            filter_text: None,
            coverage_files: Vec::new(),
            coverage_selected: 0,
            coverage_sort: CoverageSort::PercentAsc,
            coverage_source_lines: Vec::new(),
            coverage_source_scroll: 0,
            coverage_threshold: 80.0,
            status_message: String::new(),
        }
    }

    pub fn visible_nodes(&self) -> Vec<(usize, &TreeNode)> {
        self.tree.flatten()
    }

    pub fn selected_node(&self) -> Option<&TreeNode> {
        let nodes = self.visible_nodes();
        nodes.get(self.selected_index).map(|(_, n)| *n)
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.visible_nodes().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    pub fn toggle_expand(&mut self) {
        let nodes = self.tree.flatten();
        if let Some((_, node)) = nodes.get(self.selected_index) {
            let path = node.path.clone();
            let kind = node.kind.clone();
            if matches!(kind, crate::tree::node::NodeKind::Directory | crate::tree::node::NodeKind::File | crate::tree::node::NodeKind::Root) {
                self.toggle_node_at_path(&path);
            }
        }
    }

    fn toggle_node_at_path(&mut self, path: &std::path::Path) {
        Self::toggle_recursive(&mut self.tree, path);
    }

    fn toggle_recursive(node: &mut TreeNode, path: &std::path::Path) {
        if node.path == path {
            node.expanded = !node.expanded;
            return;
        }
        for child in &mut node.children {
            Self::toggle_recursive(child, path);
        }
    }

    pub fn toggle_parallel(&mut self) {
        self.parallel = !self.parallel;
    }

    pub fn toggle_watch(&mut self) {
        self.watching = !self.watching;
    }

    pub fn cycle_coverage_sort(&mut self) {
        self.coverage_sort = match self.coverage_sort {
            CoverageSort::PercentAsc => CoverageSort::PercentDesc,
            CoverageSort::PercentDesc => CoverageSort::MissesDesc,
            CoverageSort::MissesDesc => CoverageSort::FileName,
            CoverageSort::FileName => CoverageSort::PercentAsc,
        };
        self.sort_coverage();
    }

    pub fn sort_coverage(&mut self) {
        match self.coverage_sort {
            CoverageSort::PercentAsc => self.coverage_files.sort_by(|a, b| a.percent.partial_cmp(&b.percent).unwrap()),
            CoverageSort::PercentDesc => self.coverage_files.sort_by(|a, b| b.percent.partial_cmp(&a.percent).unwrap()),
            CoverageSort::MissesDesc => self.coverage_files.sort_by(|a, b| b.misses.cmp(&a.misses)),
            CoverageSort::FileName => self.coverage_files.sort_by(|a, b| a.path.cmp(&b.path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::TreeNode;
    use std::path::PathBuf;

    fn sample_tree() -> TreeNode {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut feature = TreeNode::new_directory("Feature", PathBuf::from("tests/Feature"));
        feature.add_child(TreeNode::new_test("t1", PathBuf::from("tests/Feature/t1")));
        feature.add_child(TreeNode::new_test("t2", PathBuf::from("tests/Feature/t2")));
        root.add_child(feature);
        root
    }

    #[test]
    fn test_move_up_at_zero() {
        let mut app = App::new(sample_tree(), PathBuf::from("."));
        app.move_up();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_move_down() {
        let mut app = App::new(sample_tree(), PathBuf::from("."));
        app.move_down(); // root expanded, Feature collapsed -> 2 items, move to index 1
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_move_down_clamped() {
        let mut app = App::new(sample_tree(), PathBuf::from("."));
        app.move_down();
        app.move_down();
        app.move_down();
        assert_eq!(app.selected_index, 1); // only 2 visible nodes
    }

    #[test]
    fn test_toggle_parallel() {
        let mut app = App::new(sample_tree(), PathBuf::from("."));
        assert!(app.parallel);
        app.toggle_parallel();
        assert!(!app.parallel);
    }
}
```

**Step 2: Wire up in main.rs**

```rust
mod app;
mod pest;
mod tree;

fn main() {
    println!("pesticide v0.1.0");
}
```

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: add App state with navigation and toggle logic"
```

---

### Task 6: TUI Rendering — Layout Shell

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/footer.rs`
- Modify: `src/main.rs` (add `mod ui;`, setup terminal, event loop)

**Step 1: Create footer rendering**

In `src/ui/footer.rs`:

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::app::{App, ViewMode};

pub fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let spans = match app.view_mode {
        ViewMode::Tree => vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" navigate  "),
            Span::styled("←→", Style::default().fg(Color::Yellow)),
            Span::raw(" fold  "),
            Span::styled("enter", Style::default().fg(Color::Yellow)),
            Span::raw(" run  "),
            Span::styled("a", Style::default().fg(Color::Yellow)),
            Span::raw(" all  "),
            Span::styled("c", Style::default().fg(Color::Yellow)),
            Span::raw(" coverage  "),
            Span::styled("w", Style::default().fg(Color::Yellow)),
            Span::raw(" watch  "),
            Span::styled("p", Style::default().fg(Color::Yellow)),
            Span::raw(" parallel  "),
            Span::styled("f", Style::default().fg(Color::Yellow)),
            Span::raw(" filter  "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(" quit"),
        ],
        ViewMode::CoverageTable => vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" navigate  "),
            Span::styled("enter", Style::default().fg(Color::Yellow)),
            Span::raw(" drill-in  "),
            Span::styled("s", Style::default().fg(Color::Yellow)),
            Span::raw(" sort  "),
            Span::styled("t", Style::default().fg(Color::Yellow)),
            Span::raw(" threshold  "),
            Span::styled("c", Style::default().fg(Color::Yellow)),
            Span::raw(" back to tree  "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(" quit"),
        ],
        ViewMode::CoverageSource => vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" scroll  "),
            Span::styled("n", Style::default().fg(Color::Yellow)),
            Span::raw(" next uncovered  "),
            Span::styled("esc", Style::default().fg(Color::Yellow)),
            Span::raw(" back to table  "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(" quit"),
        ],
    };

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}
```

**Step 2: Create ui/mod.rs**

In `src/ui/mod.rs`:

```rust
pub mod footer;
pub mod tree;
pub mod output;
pub mod coverage_table;
pub mod coverage_source;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::{App, ViewMode};

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),    // header
            Constraint::Min(5),       // main content (tree or coverage)
            Constraint::Length(10),   // output panel
            Constraint::Length(1),    // footer
        ])
        .split(f.area());

    render_header(f, chunks[0], app);

    match app.view_mode {
        ViewMode::Tree => tree::render_tree(f, chunks[1], app),
        ViewMode::CoverageTable => coverage_table::render_coverage_table(f, chunks[1], app),
        ViewMode::CoverageSource => coverage_source::render_coverage_source(f, chunks[1], app),
    }

    output::render_output(f, chunks[2], app);
    footer::render_footer(f, chunks[3], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let parallel_indicator = if app.parallel { "∥ parallel" } else { "→ sequential" };
    let watch_indicator = if app.watching {
        Span::styled("● watching", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ idle", Style::default().fg(Color::DarkGray))
    };

    let running_indicator = if app.running {
        Span::styled(" ⟳ running", Style::default().fg(Color::Yellow))
    } else {
        Span::raw("")
    };

    let title = format!(
        " Pesticide ─ {} ─ {} ",
        app.project_root.display(),
        parallel_indicator,
    );

    let line = Line::from(vec![
        Span::styled(title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        watch_indicator,
        running_indicator,
    ]);

    f.render_widget(Paragraph::new(line), area);
}
```

**Step 3: Create stub files for tree, output, coverage_table, coverage_source**

In `src/ui/tree.rs`:
```rust
use ratatui::{layout::Rect, Frame};
use crate::app::App;

pub fn render_tree(f: &mut Frame, area: Rect, app: &App) {
    // Will be implemented in Task 7
}
```

In `src/ui/output.rs`:
```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
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

    let text: String = app.output_lines.iter()
        .skip(app.output_scroll)
        .take(area.height as usize)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}
```

In `src/ui/coverage_table.rs`:
```rust
use ratatui::{layout::Rect, Frame};
use crate::app::App;

pub fn render_coverage_table(f: &mut Frame, area: Rect, app: &App) {
    // Will be implemented in Task 11
}
```

In `src/ui/coverage_source.rs`:
```rust
use ratatui::{layout::Rect, Frame};
use crate::app::App;

pub fn render_coverage_source(f: &mut Frame, area: Rect, app: &App) {
    // Will be implemented in Task 12
}
```

**Step 4: Wire up main.rs with terminal setup and event loop**

```rust
mod app;
mod pest;
mod tree;
mod ui;

use std::io;
use std::path::PathBuf;
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
    let project_root = discovery::find_project_root(&cwd)
        .unwrap_or_else(|| cwd.clone());

    // Discover tests
    let tree = match discovery::run_list_tests(&project_root) {
        Ok(output) => discovery::parse_test_list(&output, &project_root),
        Err(_) => {
            // No pest available, start with empty tree
            tree::node::TreeNode::new_root(project_root.join("tests"))
        }
    };

    let mut app = App::new(tree, project_root);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
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
                // Ctrl+C always quits
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
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
                app.toggle_expand(); // collapse
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if app.focus == FocusPanel::Tree {
                app.toggle_expand(); // expand
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
            // Run tests — will be implemented in Task 8
        }
        KeyCode::Char('a') => {
            // Run all tests — will be implemented in Task 8
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
        KeyCode::Enter => {
            // Drill into source — will be implemented in Task 12
            app.view_mode = ViewMode::CoverageSource;
        }
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
            if let Some(pos) = app.coverage_source_lines.iter()
                .skip(app.coverage_source_scroll + 1)
                .position(|l| l.status == crate::app::LineCoverageStatus::Uncovered)
            {
                app.coverage_source_scroll += pos + 1;
            }
        }
        _ => {}
    }
}
```

**Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

**Step 6: Commit**

```bash
git add src/
git commit -m "feat: add TUI layout shell with header, output, footer, and event loop"
```

---

### Task 7: Tree Widget Rendering

**Files:**
- Modify: `src/ui/tree.rs`

**Step 1: Implement tree rendering**

Replace `src/ui/tree.rs` with:

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::App;
use crate::tree::node::{NodeKind, TestStatus};

pub fn render_tree(f: &mut Frame, area: Rect, app: &App) {
    let nodes = app.visible_nodes();
    let height = area.height as usize;

    // Determine scroll offset to keep selected item visible
    let scroll = if app.selected_index >= height {
        app.selected_index - height + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();

    for (i, (depth, node)) in nodes.iter().enumerate().skip(scroll).take(height) {
        let is_selected = i == app.selected_index;
        let indent = "  ".repeat(*depth);

        // Expand/collapse indicator
        let expand_icon = match node.kind {
            NodeKind::Root | NodeKind::Directory | NodeKind::File => {
                if node.expanded { "▼ " } else { "▶ " }
            }
            NodeKind::Test => "  ",
        };

        // Status icon
        let aggregate = node.aggregate_status();
        let (status_icon, status_color) = match aggregate {
            TestStatus::Passed => ("✓ ", Color::Green),
            TestStatus::Failed => ("✗ ", Color::Red),
            TestStatus::Running => ("⟳ ", Color::Yellow),
            TestStatus::NotRun => ("◌ ", Color::DarkGray),
        };

        // Test count suffix for non-test nodes
        let count_suffix = match node.kind {
            NodeKind::Test => String::new(),
            _ if node.test_count > 0 => format!(" ({})", node.test_count),
            _ => String::new(),
        };

        // Coverage suffix
        let coverage_suffix = match node.coverage_percent {
            Some(pct) => {
                let color = if pct >= 80.0 {
                    Color::Green
                } else if pct >= 50.0 {
                    Color::Yellow
                } else {
                    Color::Red
                };
                (format!(" {:>5.1}%", pct), color)
            }
            None => (String::new(), Color::DarkGray),
        };

        let base_style = if is_selected {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let mut spans = vec![
            Span::styled(indent.clone(), base_style),
            Span::styled(status_icon, base_style.fg(status_color)),
            Span::styled(expand_icon, base_style.fg(Color::White)),
            Span::styled(&node.name, base_style.fg(Color::White)),
            Span::styled(count_suffix, base_style.fg(Color::DarkGray)),
        ];

        if !coverage_suffix.0.is_empty() {
            // Pad to right-align coverage
            let used = indent.len() + 2 + 2 + node.name.len() + spans[4].content.len();
            let remaining = (area.width as usize).saturating_sub(used + coverage_suffix.0.len());
            let padding = " ".repeat(remaining);
            spans.push(Span::styled(padding, base_style));
            spans.push(Span::styled(coverage_suffix.0, base_style.fg(coverage_suffix.1)));
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

**Step 3: Commit**

```bash
git add src/ui/tree.rs
git commit -m "feat: add tree widget rendering with status icons and coverage display"
```

---

### Task 8: Test Runner — Spawn Pest and Stream Output

**Files:**
- Create: `src/pest/runner.rs`
- Modify: `src/pest/mod.rs`
- Modify: `src/main.rs` (wire up Enter/a keys)

**Step 1: Implement runner**

In `src/pest/runner.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use anyhow::Result;

use crate::tree::node::TestStatus;

#[derive(Debug, Clone)]
pub enum RunScope {
    All,
    File(PathBuf),
    Directory(PathBuf),
    Test { file: PathBuf, name: String },
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
}

pub struct RunHandle {
    pub child: tokio::process::Child,
}

impl RunHandle {
    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }
}

pub fn build_pest_command(
    project_root: &Path,
    scope: &RunScope,
    parallel: bool,
    coverage: bool,
) -> Command {
    let pest_bin = project_root.join("vendor/bin/pest");
    let mut cmd = Command::new(pest_bin);
    cmd.current_dir(project_root);

    match scope {
        RunScope::All => {}
        RunScope::File(path) => {
            cmd.arg(path);
        }
        RunScope::Directory(path) => {
            cmd.arg(path);
        }
        RunScope::Test { file, name } => {
            cmd.arg(file);
            cmd.arg("--filter");
            cmd.arg(name);
        }
    }

    if parallel {
        cmd.arg("--parallel");
    }

    // Use TeamCity format for structured parsing
    cmd.arg("--teamcity");

    if coverage {
        let coverage_path = project_root.join(".pesticide/coverage.xml");
        cmd.arg("--coverage-clover");
        cmd.arg(coverage_path);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd
}

pub async fn run_tests(
    project_root: &Path,
    scope: RunScope,
    parallel: bool,
    coverage: bool,
    output_lines: Arc<Mutex<Vec<String>>>,
    results: Arc<Mutex<Vec<TestResult>>>,
) -> Result<RunHandle> {
    // Ensure .pesticide dir exists
    let pesticide_dir = project_root.join(".pesticide");
    tokio::fs::create_dir_all(&pesticide_dir).await.ok();

    let mut cmd = build_pest_command(project_root, &scope, parallel, coverage);
    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let output_lines_clone = output_lines.clone();
    let results_clone = results.clone();

    // Stream stdout
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // Parse TeamCity messages for test results
            if line.starts_with("##teamcity[testFinished") {
                if let Some(name) = extract_teamcity_attr(&line, "name") {
                    results_clone.lock().unwrap().push(TestResult {
                        name,
                        status: TestStatus::Passed,
                    });
                }
            } else if line.starts_with("##teamcity[testFailed") {
                if let Some(name) = extract_teamcity_attr(&line, "name") {
                    results_clone.lock().unwrap().push(TestResult {
                        name,
                        status: TestStatus::Failed,
                    });
                }
            }
            output_lines_clone.lock().unwrap().push(line);
        }
    });

    let output_lines_clone2 = output_lines.clone();

    // Stream stderr
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            output_lines_clone2.lock().unwrap().push(line);
        }
    });

    Ok(RunHandle { child })
}

fn extract_teamcity_attr(line: &str, attr: &str) -> Option<String> {
    let search = format!("{}='", attr);
    let start = line.find(&search)? + search.len();
    let rest = &line[start..];
    // TeamCity escapes: |' for ', |n for newline, || for |, |[ for [, |] for ]
    let mut result = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '\'' {
            break;
        }
        if c == '|' {
            match chars.next() {
                Some('\'') => result.push('\''),
                Some('n') => result.push('\n'),
                Some('|') => result.push('|'),
                Some('[') => result.push('['),
                Some(']') => result.push(']'),
                Some(other) => result.push(other),
                None => break,
            }
        } else {
            result.push(c);
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_teamcity_attr() {
        let line = "##teamcity[testFinished name='it can login' duration='42']";
        assert_eq!(extract_teamcity_attr(line, "name"), Some("it can login".to_string()));
        assert_eq!(extract_teamcity_attr(line, "duration"), Some("42".to_string()));
    }

    #[test]
    fn test_extract_teamcity_attr_escaped() {
        let line = "##teamcity[testFailed name='it|'s a test' message='fail']";
        assert_eq!(extract_teamcity_attr(line, "name"), Some("it's a test".to_string()));
    }

    #[test]
    fn test_extract_teamcity_attr_missing() {
        let line = "##teamcity[testFinished name='test']";
        assert_eq!(extract_teamcity_attr(line, "missing"), None);
    }

    #[test]
    fn test_build_command_all_parallel() {
        let cmd = build_pest_command(
            Path::new("/project"),
            &RunScope::All,
            true,
            false,
        );
        let prog = cmd.as_std().get_program().to_str().unwrap().to_string();
        assert!(prog.contains("vendor/bin/pest"));
        let args: Vec<_> = cmd.as_std().get_args().map(|a| a.to_str().unwrap().to_string()).collect();
        assert!(args.contains(&"--parallel".to_string()));
        assert!(args.contains(&"--teamcity".to_string()));
    }

    #[test]
    fn test_build_command_file_no_parallel() {
        let cmd = build_pest_command(
            Path::new("/project"),
            &RunScope::File(PathBuf::from("tests/Feature/LoginTest.php")),
            false,
            false,
        );
        let args: Vec<_> = cmd.as_std().get_args().map(|a| a.to_str().unwrap().to_string()).collect();
        assert!(args.contains(&"tests/Feature/LoginTest.php".to_string()));
        assert!(!args.contains(&"--parallel".to_string()));
    }

    #[test]
    fn test_build_command_with_coverage() {
        let cmd = build_pest_command(
            Path::new("/project"),
            &RunScope::All,
            true,
            true,
        );
        let args: Vec<_> = cmd.as_std().get_args().map(|a| a.to_str().unwrap().to_string()).collect();
        assert!(args.contains(&"--coverage-clover".to_string()));
    }
}
```

**Step 2: Update pest/mod.rs**

```rust
pub mod discovery;
pub mod runner;
```

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add src/pest/runner.rs src/pest/mod.rs
git commit -m "feat: add test runner with TeamCity output parsing and process management"
```

---

### Task 9: Wire Runner Into Event Loop

**Files:**
- Modify: `src/main.rs` (make async, wire Enter/a keys to runner)
- Modify: `src/app.rs` (add shared state for async output)

**Step 1: Add shared state to App**

Add to `src/app.rs`:

```rust
use std::sync::{Arc, Mutex};
use crate::pest::runner::TestResult;
```

Add fields to `App`:
```rust
    pub shared_output: Arc<Mutex<Vec<String>>>,
    pub shared_results: Arc<Mutex<Vec<TestResult>>>,
```

Update `App::new()` to initialize them:
```rust
    shared_output: Arc::new(Mutex::new(Vec::new())),
    shared_results: Arc::new(Mutex::new(Vec::new())),
```

Add a method to `App`:
```rust
    /// Sync shared async output into app state.
    pub fn sync_output(&mut self) {
        let mut shared = self.shared_output.lock().unwrap();
        if !shared.is_empty() {
            self.output_lines.append(&mut *shared);
        }

        let mut results = self.shared_results.lock().unwrap();
        for result in results.drain(..) {
            self.apply_test_result(&result);
        }
    }

    fn apply_test_result(&mut self, result: &TestResult) {
        Self::apply_result_recursive(&mut self.tree, result);
    }

    fn apply_result_recursive(node: &mut TreeNode, result: &TestResult) {
        if node.kind == NodeKind::Test && node.name == result.name {
            node.status = result.status.clone();
            return;
        }
        for child in &mut node.children {
            Self::apply_result_recursive(child, result);
        }
    }
```

**Step 2: Make main async and wire keys to runner**

Update `main.rs` — change `fn main()` to `#[tokio::main] async fn main()` and add RunScope logic to `handle_tree_keys`. Store a `Option<RunHandle>` as mutable state passed through the loop.

The key wiring for Enter:
```rust
KeyCode::Enter => {
    if let Some(node) = app.selected_node().cloned() {
        let scope = match node.kind {
            NodeKind::Test => RunScope::Test {
                file: node.path.clone(),
                name: node.name.clone(),
            },
            NodeKind::File => RunScope::File(node.path.clone()),
            NodeKind::Directory | NodeKind::Root => RunScope::Directory(node.path.clone()),
        };
        // Clear output, start run
        app.output_lines.clear();
        app.output_scroll = 0;
        *app.shared_output.lock().unwrap() = Vec::new();
        *app.shared_results.lock().unwrap() = Vec::new();
        app.running = true;
        // spawn run (details in implementation)
    }
}
```

**Step 3: Add tick-based sync in main loop**

In `run_loop`, after polling events, call `app.sync_output()` every tick to pull async output into the app.

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 5: Commit**

```bash
git add src/main.rs src/app.rs
git commit -m "feat: wire test runner into event loop with async output streaming"
```

---

### Task 10: Coverage XML Parsing

**Files:**
- Create: `src/pest/coverage.rs`
- Modify: `src/pest/mod.rs`

**Step 1: Implement Clover XML parser**

In `src/pest/coverage.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;
use crate::app::{CoverageFile, CoverageSourceLine, LineCoverageStatus};

/// Parse a Clover XML coverage report.
pub fn parse_clover_xml(xml: &str) -> Result<Vec<CoverageFile>> {
    let doc = roxmltree::Document::parse(xml)?;
    let mut files = Vec::new();

    for file_node in doc.descendants().filter(|n| n.has_tag_name("file")) {
        let path = file_node.attribute("name").unwrap_or("").to_string();
        if path.is_empty() {
            continue;
        }

        let mut lines_total = 0usize;
        let mut lines_hit = 0usize;

        for line in file_node.children().filter(|n| n.has_tag_name("line")) {
            let line_type = line.attribute("type").unwrap_or("");
            if line_type == "stmt" || line_type == "method" || line_type == "cond" {
                lines_total += 1;
                let count: usize = line.attribute("count").unwrap_or("0").parse().unwrap_or(0);
                if count > 0 {
                    lines_hit += 1;
                }
            }
        }

        let misses = lines_total.saturating_sub(lines_hit);
        let percent = if lines_total > 0 {
            (lines_hit as f64 / lines_total as f64) * 100.0
        } else {
            100.0
        };

        files.push(CoverageFile {
            path,
            lines: lines_total,
            hits: lines_hit,
            misses,
            percent,
        });
    }

    Ok(files)
}

/// Parse line-level coverage for a specific file from Clover XML.
/// Returns a map of line_number -> hit_count.
pub fn parse_file_line_coverage(xml: &str, file_path: &str) -> Result<HashMap<usize, usize>> {
    let doc = roxmltree::Document::parse(xml)?;
    let mut line_hits = HashMap::new();

    for file_node in doc.descendants().filter(|n| n.has_tag_name("file")) {
        let path = file_node.attribute("name").unwrap_or("");
        if path != file_path {
            continue;
        }

        for line in file_node.children().filter(|n| n.has_tag_name("line")) {
            if let Some(num_str) = line.attribute("num") {
                let num: usize = num_str.parse().unwrap_or(0);
                let count: usize = line.attribute("count").unwrap_or("0").parse().unwrap_or(0);
                line_hits.insert(num, count);
            }
        }
    }

    Ok(line_hits)
}

/// Build coverage source lines by reading the file and merging with coverage data.
pub fn build_coverage_source(
    source_path: &Path,
    line_hits: &HashMap<usize, usize>,
) -> Result<Vec<CoverageSourceLine>> {
    let content = std::fs::read_to_string(source_path)?;
    let mut lines = Vec::new();

    for (i, line_content) in content.lines().enumerate() {
        let line_num = i + 1;
        let status = match line_hits.get(&line_num) {
            Some(&count) if count > 0 => LineCoverageStatus::Covered,
            Some(_) => LineCoverageStatus::Uncovered,
            None => LineCoverageStatus::NotExecutable,
        };
        lines.push(CoverageSourceLine {
            line_number: line_num,
            content: line_content.to_string(),
            status,
        });
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CLOVER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<coverage generated="1234567890">
  <project timestamp="1234567890">
    <file name="app/Models/Player.php">
      <line num="10" type="stmt" count="5"/>
      <line num="11" type="stmt" count="5"/>
      <line num="12" type="stmt" count="0"/>
      <line num="20" type="method" count="3"/>
    </file>
    <file name="app/Services/AuthService.php">
      <line num="5" type="stmt" count="10"/>
      <line num="6" type="stmt" count="0"/>
      <line num="7" type="stmt" count="0"/>
    </file>
  </project>
</coverage>"#;

    #[test]
    fn test_parse_clover_xml() {
        let files = parse_clover_xml(SAMPLE_CLOVER).unwrap();
        assert_eq!(files.len(), 2);

        let player = &files[0];
        assert_eq!(player.path, "app/Models/Player.php");
        assert_eq!(player.lines, 4);
        assert_eq!(player.hits, 3);
        assert_eq!(player.misses, 1);
        assert!((player.percent - 75.0).abs() < 0.1);

        let auth = &files[1];
        assert_eq!(auth.path, "app/Services/AuthService.php");
        assert_eq!(auth.lines, 3);
        assert_eq!(auth.hits, 1);
        assert_eq!(auth.misses, 2);
    }

    #[test]
    fn test_parse_file_line_coverage() {
        let hits = parse_file_line_coverage(SAMPLE_CLOVER, "app/Models/Player.php").unwrap();
        assert_eq!(hits.get(&10), Some(&5));
        assert_eq!(hits.get(&12), Some(&0));
        assert_eq!(hits.get(&99), None);
    }

    #[test]
    fn test_parse_empty_xml() {
        let xml = r#"<?xml version="1.0"?><coverage><project></project></coverage>"#;
        let files = parse_clover_xml(xml).unwrap();
        assert!(files.is_empty());
    }
}
```

**Step 2: Update pest/mod.rs**

```rust
pub mod coverage;
pub mod discovery;
pub mod runner;
```

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add src/pest/coverage.rs src/pest/mod.rs
git commit -m "feat: add Clover XML coverage parsing with file and line-level data"
```

---

### Task 11: Coverage Table Rendering

**Files:**
- Modify: `src/ui/coverage_table.rs`

**Step 1: Implement coverage table widget**

Replace `src/ui/coverage_table.rs`:

```rust
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
    let mut lines: Vec<Line> = Vec::new();

    // Header
    let sort_label = match app.coverage_sort {
        CoverageSort::PercentAsc => "% ↑",
        CoverageSort::PercentDesc => "% ↓",
        CoverageSort::MissesDesc => "Miss ↓",
        CoverageSort::FileName => "Name",
    };

    // Total coverage
    let total_lines: usize = app.coverage_files.iter().map(|f| f.lines).sum();
    let total_hits: usize = app.coverage_files.iter().map(|f| f.hits).sum();
    let total_pct = if total_lines > 0 { (total_hits as f64 / total_lines as f64) * 100.0 } else { 0.0 };

    lines.push(Line::from(vec![
        Span::styled(
            format!(" Coverage ─ {:.1}% total ─ sorted by: {} ", total_pct, sort_label),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Column headers
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {:<45} {:>6} {:>6} {:>6} {:>6}", "File", "Lines", "Hit", "Miss", "%"),
            Style::default().add_modifier(Modifier::UNDERLINED).fg(Color::White),
        ),
    ]));

    // File rows
    for (i, cov_file) in app.coverage_files.iter().enumerate().take(height.saturating_sub(3)) {
        let is_selected = i == app.coverage_selected;
        let below_threshold = cov_file.percent < app.coverage_threshold;

        let pct_color = if cov_file.percent >= 80.0 {
            Color::Green
        } else if cov_file.percent >= 50.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        let base_style = if is_selected {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else if below_threshold {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };

        // Truncate long paths
        let display_path = if cov_file.path.len() > 45 {
            format!("…{}", &cov_file.path[cov_file.path.len() - 44..])
        } else {
            cov_file.path.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<45} {:>6} {:>6} {:>6}", display_path, cov_file.lines, cov_file.hits, cov_file.misses),
                base_style,
            ),
            Span::styled(
                format!(" {:>5.1}%", cov_file.percent),
                base_style.fg(pct_color),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 3: Commit**

```bash
git add src/ui/coverage_table.rs
git commit -m "feat: add coverage table rendering with sort indicators and threshold highlighting"
```

---

### Task 12: Coverage Source View Rendering

**Files:**
- Modify: `src/ui/coverage_source.rs`

**Step 1: Implement coverage source widget**

Replace `src/ui/coverage_source.rs`:

```rust
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
    let mut lines: Vec<Line> = Vec::new();

    // Header with file info
    if let Some(cov_file) = app.coverage_files.get(app.coverage_selected) {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ─ {:.1}%", cov_file.path, cov_file.percent),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let visible_lines = app.coverage_source_lines.iter()
        .skip(app.coverage_source_scroll)
        .take(height.saturating_sub(1));

    for source_line in visible_lines {
        let (marker, bg_color) = match source_line.status {
            LineCoverageStatus::Covered => ("██", Color::Green),
            LineCoverageStatus::Uncovered => ("░░", Color::Red),
            LineCoverageStatus::NotExecutable => ("  ", Color::Reset),
        };

        let line_style = match source_line.status {
            LineCoverageStatus::Covered => Style::default().bg(Color::Rgb(0, 40, 0)),
            LineCoverageStatus::Uncovered => Style::default().bg(Color::Rgb(40, 0, 0)),
            LineCoverageStatus::NotExecutable => Style::default(),
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("{:>4} ", source_line.line_number),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{} ", marker),
                Style::default().fg(bg_color),
            ),
            Span::styled(
                source_line.content.clone(),
                line_style,
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 3: Commit**

```bash
git add src/ui/coverage_source.rs
git commit -m "feat: add coverage source view with line-level highlighting"
```

---

### Task 13: File Watcher

**Files:**
- Create: `src/watcher.rs`
- Modify: `src/main.rs` (wire watcher into event loop)

**Step 1: Implement file watcher**

In `src/watcher.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use anyhow::Result;

pub enum WatchEvent {
    TestFileChanged(PathBuf),
    SourceFileChanged(PathBuf),
    TestFileCreatedOrDeleted,
}

pub fn start_watcher(
    project_root: &Path,
    tx: mpsc::Sender<WatchEvent>,
) -> Result<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
    let project_root = project_root.to_path_buf();

    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            let events = match events {
                Ok(e) => e,
                Err(_) => return,
            };

            for event in events {
                if event.kind != DebouncedEventKind::Any {
                    continue;
                }

                let path = &event.path;
                let path_str = path.to_string_lossy();

                // Skip ignored directories
                if path_str.contains(".git/")
                    || path_str.contains("vendor/")
                    || path_str.contains("node_modules/")
                    || path_str.contains(".pesticide/")
                {
                    continue;
                }

                // Determine event type
                if path_str.contains("/tests/") || path_str.contains("/app-modules/") && path_str.contains("/tests/") {
                    if path.exists() {
                        let _ = tx.send(WatchEvent::TestFileChanged(path.clone()));
                    } else {
                        let _ = tx.send(WatchEvent::TestFileCreatedOrDeleted);
                    }
                } else if path_str.contains("/app/") {
                    let _ = tx.send(WatchEvent::SourceFileChanged(path.clone()));
                }
            }
        },
    )?;

    // Watch the project root recursively
    debouncer.watcher().watch(
        &project_root,
        notify::RecursiveMode::Recursive,
    )?;

    Ok(debouncer)
}
```

**Step 2: Wire up in main.rs**

In the main loop, check for `WatchEvent`s from the mpsc channel and trigger appropriate runs.

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 4: Commit**

```bash
git add src/watcher.rs src/main.rs
git commit -m "feat: add file watcher with debounce for tests/ and app/ directories"
```

---

### Task 14: Wire Coverage Into App Flow

**Files:**
- Modify: `src/main.rs` (add 'c' key to trigger coverage run, load results)
- Modify: `src/app.rs` (add method to load coverage data)

**Step 1: Add coverage loading to App**

Add to `src/app.rs`:

```rust
use crate::pest::coverage;

impl App {
    pub fn load_coverage(&mut self) -> Result<(), anyhow::Error> {
        let coverage_path = self.project_root.join(".pesticide/coverage.xml");
        let xml = std::fs::read_to_string(&coverage_path)?;
        self.coverage_files = coverage::parse_clover_xml(&xml)?;
        self.sort_coverage();
        Ok(())
    }

    pub fn load_coverage_source(&mut self) -> Result<(), anyhow::Error> {
        if let Some(cov_file) = self.coverage_files.get(self.coverage_selected) {
            let xml_path = self.project_root.join(".pesticide/coverage.xml");
            let xml = std::fs::read_to_string(&xml_path)?;
            let line_hits = coverage::parse_file_line_coverage(&xml, &cov_file.path)?;
            let source_path = self.project_root.join(&cov_file.path);
            self.coverage_source_lines = coverage::build_coverage_source(&source_path, &line_hits)?;
            self.coverage_source_scroll = 0;
        }
        Ok(())
    }
}
```

**Step 2: Wire 'c' key to run tests with coverage, then switch to coverage view**

In `handle_tree_keys`, pressing `c` should:
1. Run all tests with `coverage: true`
2. When run completes, call `app.load_coverage()`
3. Switch to `ViewMode::CoverageTable`

In `handle_coverage_table_keys`, pressing Enter should:
1. Call `app.load_coverage_source()`
2. Switch to `ViewMode::CoverageSource`

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 4: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: wire coverage generation and viewing into app flow"
```

---

### Task 15: Filter/Search

**Files:**
- Modify: `src/app.rs` (add filter state and logic)
- Modify: `src/main.rs` (handle 'f' key, text input mode)

**Step 1: Add filter logic to App**

Add to `src/app.rs`:

```rust
impl App {
    pub fn visible_nodes(&self) -> Vec<(usize, &TreeNode)> {
        let all = self.tree.flatten();
        match &self.filter_text {
            Some(filter) if !filter.is_empty() => {
                let filter_lower = filter.to_lowercase();
                all.into_iter()
                    .filter(|(_, node)| node.name.to_lowercase().contains(&filter_lower))
                    .collect()
            }
            _ => all,
        }
    }
}
```

**Step 2: Add text input handling for filter mode**

When `f` is pressed, enter a filter input mode where keystrokes build `filter_text`. `Esc` exits filter mode, `Enter` confirms.

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 4: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: add filter/search for test tree"
```

---

### Task 16: CLI Arguments

**Files:**
- Modify: `src/main.rs`

**Step 1: Add clap argument parsing**

```rust
use clap::Parser;

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
}
```

**Step 2: Wire arguments into App initialization**

```rust
let cli = Cli::parse();
// Use cli.path, cli.no_parallel, cli.watch when creating App
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add CLI argument parsing with clap"
```

---

### Task 17: Integration Test With Real Laravel Project

**Files:** None (manual testing)

**Step 1: Build release binary**

Run: `cargo build --release`

**Step 2: Test against quantiforme**

Run:
```bash
cd /Users/nhinkley/Repos/quantiforme
/Users/nhinkley/Repos/Pesticide/target/release/pesticide
```

**Step 3: Verify**

- Tree populates with tests from `--list-tests`
- Navigation works (j/k/arrows, expand/collapse)
- Running a single test (Enter) works
- Running all tests (a) works
- Parallel toggle (p) works
- Coverage mode (c) generates and displays coverage
- Watch mode (w) picks up file changes
- Filter (f) narrows the tree
- q quits cleanly

**Step 4: Fix any issues found**

**Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration test fixes from real project testing"
```

---

### Task 18: Add .gitignore and README

**Files:**
- Create: `.gitignore`

**Step 1: Create .gitignore**

```
/target
.pesticide/
```

**Step 2: Commit**

```bash
git add .gitignore
git commit -m "chore: add .gitignore"
```
