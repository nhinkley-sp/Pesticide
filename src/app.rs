use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::pest::runner::TestResult;
use crate::tree::node::{NodeKind, TreeNode};

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

#[derive(Debug)]
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
    pub shared_output: Arc<Mutex<Vec<String>>>,
    pub shared_results: Arc<Mutex<Vec<TestResult>>>,
}

impl App {
    /// Creates a new App with default values.
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
            shared_output: Arc::new(Mutex::new(Vec::new())),
            shared_results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns a flat list of `(depth, &TreeNode)` for all visible (expanded) nodes.
    pub fn visible_nodes(&self) -> Vec<(usize, &TreeNode)> {
        self.tree.flatten()
    }

    /// Returns the currently selected node, if any.
    pub fn selected_node(&self) -> Option<&TreeNode> {
        let nodes = self.visible_nodes();
        nodes.get(self.selected_index).map(|(_, node)| *node)
    }

    /// Move selection up by one (clamped at 0).
    pub fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    /// Move selection down by one (clamped at max visible index).
    pub fn move_down(&mut self) {
        let max = self.visible_nodes().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }

    /// Toggle expand/collapse on the selected node (only Root, Directory, or File).
    pub fn toggle_expand(&mut self) {
        let visible = self.tree.flatten();
        if let Some(&(_, node)) = visible.get(self.selected_index) {
            match node.kind {
                NodeKind::Root | NodeKind::Directory | NodeKind::File => {
                    // We need to find and mutate the node in the tree.
                    // Use the node's path and kind to locate it.
                    let path = node.path.clone();
                    let kind = node.kind.clone();
                    let name = node.name.clone();
                    Self::toggle_node_in_tree(&mut self.tree, &path, &kind, &name);
                }
                NodeKind::Test => {}
            }
        }
    }

    /// Recursively find and toggle the expanded state of a node matching path, kind, and name.
    fn toggle_node_in_tree(node: &mut TreeNode, path: &PathBuf, kind: &NodeKind, name: &str) -> bool {
        if node.path == *path && node.kind == *kind && node.name == name {
            node.expanded = !node.expanded;
            return true;
        }
        for child in &mut node.children {
            if Self::toggle_node_in_tree(child, path, kind, name) {
                return true;
            }
        }
        false
    }

    /// Toggle parallel mode.
    pub fn toggle_parallel(&mut self) {
        self.parallel = !self.parallel;
    }

    /// Toggle watch mode.
    pub fn toggle_watch(&mut self) {
        self.watching = !self.watching;
    }

    /// Drains shared_output into output_lines and shared_results into apply_test_result.
    pub fn sync_output(&mut self) {
        // Drain shared output lines
        if let Ok(mut lines) = self.shared_output.lock() {
            self.output_lines.append(&mut *lines);
        }

        // Drain shared results and apply each
        let results: Vec<TestResult> = {
            if let Ok(mut res) = self.shared_results.lock() {
                res.drain(..).collect()
            } else {
                Vec::new()
            }
        };
        for result in &results {
            self.apply_test_result(result);
        }
    }

    /// Recursively walks the tree to find a test node matching `result.name` and updates its status.
    pub fn apply_test_result(&mut self, result: &TestResult) {
        Self::apply_result_to_node(&mut self.tree, result);
    }

    fn apply_result_to_node(node: &mut TreeNode, result: &TestResult) -> bool {
        if node.kind == NodeKind::Test && node.name == result.name {
            node.status = result.status.clone();
            return true;
        }
        for child in &mut node.children {
            if Self::apply_result_to_node(child, result) {
                return true;
            }
        }
        false
    }

    /// Cycle through coverage sort modes:
    /// PercentAsc -> PercentDesc -> MissesDesc -> FileName -> PercentAsc
    pub fn cycle_coverage_sort(&mut self) {
        self.coverage_sort = match self.coverage_sort {
            CoverageSort::PercentAsc => CoverageSort::PercentDesc,
            CoverageSort::PercentDesc => CoverageSort::MissesDesc,
            CoverageSort::MissesDesc => CoverageSort::FileName,
            CoverageSort::FileName => CoverageSort::PercentAsc,
        };
        self.sort_coverage();
    }

    /// Sort coverage_files according to the current coverage_sort mode.
    pub fn sort_coverage(&mut self) {
        match self.coverage_sort {
            CoverageSort::PercentAsc => {
                self.coverage_files
                    .sort_by(|a, b| a.percent.partial_cmp(&b.percent).unwrap_or(std::cmp::Ordering::Equal));
            }
            CoverageSort::PercentDesc => {
                self.coverage_files
                    .sort_by(|a, b| b.percent.partial_cmp(&a.percent).unwrap_or(std::cmp::Ordering::Equal));
            }
            CoverageSort::MissesDesc => {
                self.coverage_files.sort_by(|a, b| b.misses.cmp(&a.misses));
            }
            CoverageSort::FileName => {
                self.coverage_files.sort_by(|a, b| a.path.cmp(&b.path));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a simple tree with root + directory (expanded) + 2 tests.
    fn make_test_tree() -> TreeNode {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut dir = TreeNode::new_directory(
            "Feature".to_string(),
            PathBuf::from("tests/Feature"),
        );
        dir.expanded = true;
        let test1 = TreeNode::new_test(
            "it works".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        let test2 = TreeNode::new_test(
            "it also works".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        dir.add_child(test1);
        dir.add_child(test2);
        root.add_child(dir);
        root
    }

    #[test]
    fn test_move_up_at_zero() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        assert_eq!(app.selected_index, 0);
        app.move_up();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_move_down() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        assert_eq!(app.selected_index, 0);
        app.move_down();
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_move_down_clamped() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        // visible nodes: root, Feature (expanded), test1, test2 = 4 nodes, max index = 3
        let max = app.visible_nodes().len() - 1;
        app.selected_index = max;
        app.move_down();
        assert_eq!(app.selected_index, max);
    }

    #[test]
    fn test_toggle_parallel() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        assert!(app.parallel);
        app.toggle_parallel();
        assert!(!app.parallel);
        app.toggle_parallel();
        assert!(app.parallel);
    }

    #[test]
    fn test_toggle_expand() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        // Initially: root(expanded), Feature(expanded), test1, test2 => 4 visible
        assert_eq!(app.visible_nodes().len(), 4);

        // Select the Feature directory (index 1) and collapse it
        app.selected_index = 1;
        app.toggle_expand();
        // Now: root(expanded), Feature(collapsed) => 2 visible
        assert_eq!(app.visible_nodes().len(), 2);

        // Expand it again
        app.toggle_expand();
        assert_eq!(app.visible_nodes().len(), 4);
    }

    #[test]
    fn test_toggle_watch() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        assert!(!app.watching);
        app.toggle_watch();
        assert!(app.watching);
    }

    #[test]
    fn test_cycle_coverage_sort() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        assert_eq!(app.coverage_sort, CoverageSort::PercentAsc);
        app.cycle_coverage_sort();
        assert_eq!(app.coverage_sort, CoverageSort::PercentDesc);
        app.cycle_coverage_sort();
        assert_eq!(app.coverage_sort, CoverageSort::MissesDesc);
        app.cycle_coverage_sort();
        assert_eq!(app.coverage_sort, CoverageSort::FileName);
        app.cycle_coverage_sort();
        assert_eq!(app.coverage_sort, CoverageSort::PercentAsc);
    }

    #[test]
    fn test_sort_coverage() {
        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        app.coverage_files = vec![
            CoverageFile {
                path: "b.php".to_string(),
                lines: 100,
                hits: 80,
                misses: 20,
                percent: 80.0,
            },
            CoverageFile {
                path: "a.php".to_string(),
                lines: 50,
                hits: 25,
                misses: 25,
                percent: 50.0,
            },
            CoverageFile {
                path: "c.php".to_string(),
                lines: 200,
                hits: 190,
                misses: 10,
                percent: 95.0,
            },
        ];

        // Default sort is PercentAsc
        app.sort_coverage();
        assert_eq!(app.coverage_files[0].path, "a.php");
        assert_eq!(app.coverage_files[1].path, "b.php");
        assert_eq!(app.coverage_files[2].path, "c.php");

        // PercentDesc
        app.coverage_sort = CoverageSort::PercentDesc;
        app.sort_coverage();
        assert_eq!(app.coverage_files[0].path, "c.php");
        assert_eq!(app.coverage_files[2].path, "a.php");

        // MissesDesc
        app.coverage_sort = CoverageSort::MissesDesc;
        app.sort_coverage();
        assert_eq!(app.coverage_files[0].path, "a.php"); // 25 misses
        assert_eq!(app.coverage_files[1].path, "b.php"); // 20 misses
        assert_eq!(app.coverage_files[2].path, "c.php"); // 10 misses

        // FileName
        app.coverage_sort = CoverageSort::FileName;
        app.sort_coverage();
        assert_eq!(app.coverage_files[0].path, "a.php");
        assert_eq!(app.coverage_files[1].path, "b.php");
        assert_eq!(app.coverage_files[2].path, "c.php");
    }

    #[test]
    fn test_selected_node() {
        let tree = make_test_tree();
        let app = App::new(tree, PathBuf::from("/project"));
        let node = app.selected_node().unwrap();
        assert_eq!(node.kind, NodeKind::Root);
    }

    #[test]
    fn test_defaults() {
        let tree = TreeNode::new_root(PathBuf::from("tests"));
        let app = App::new(tree, PathBuf::from("/project"));
        assert_eq!(app.selected_index, 0);
        assert!(app.output_lines.is_empty());
        assert_eq!(app.output_scroll, 0);
        assert_eq!(app.view_mode, ViewMode::Tree);
        assert_eq!(app.focus, FocusPanel::Tree);
        assert!(app.parallel);
        assert!(!app.watching);
        assert!(!app.running);
        assert!(!app.should_quit);
        assert!(app.filter_text.is_none());
        assert!(app.coverage_files.is_empty());
        assert_eq!(app.coverage_selected, 0);
        assert_eq!(app.coverage_sort, CoverageSort::PercentAsc);
        assert!(app.coverage_source_lines.is_empty());
        assert_eq!(app.coverage_source_scroll, 0);
        assert_eq!(app.coverage_threshold, 80.0);
        assert!(app.status_message.is_empty());
    }
}
