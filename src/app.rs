use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Context;

use crate::pest::coverage;
use crate::pest::runner::TestResult;
use crate::tree::node::{NodeKind, TreeNode};

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    Tree,
    CoverageTable,
    CoverageTree,
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

#[derive(Debug, Clone)]
pub struct CoverageTreeNode {
    pub name: String,
    pub path: String,
    pub is_file: bool,
    pub expanded: bool,
    pub children: Vec<CoverageTreeNode>,
    pub lines: usize,
    pub hits: usize,
    pub misses: usize,
    pub percent: f64,
}

impl CoverageTreeNode {
    /// Returns a flat list of visible (expanded) nodes with their depth.
    pub fn flatten(&self, depth: usize) -> Vec<(usize, &CoverageTreeNode)> {
        let mut result = vec![(depth, self)];
        if self.expanded {
            for child in &self.children {
                result.extend(child.flatten(depth + 1));
            }
        }
        result
    }
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
    pub filter_active: bool,
    pub filter_text: Option<String>,
    pub coverage_files: Vec<CoverageFile>,
    pub coverage_selected: usize,
    pub coverage_sort: CoverageSort,
    pub coverage_source_lines: Vec<CoverageSourceLine>,
    pub coverage_source_scroll: usize,
    pub coverage_threshold: f64,
    pub coverage_pending: bool,
    pub coverage_drill_pending: bool,
    pub coverage_tree_root: Option<CoverageTreeNode>,
    pub coverage_tree_selected: usize,
    /// When true, a watch event fired while tests were already running.
    /// The run will be re-triggered once the current run finishes.
    pub rerun_pending: bool,
    pub status_message: String,
    pub shared_output: Arc<Mutex<Vec<String>>>,
    pub shared_results: Arc<Mutex<Vec<TestResult>>>,
    /// Monotonically increasing tick counter for driving animations (throbber).
    pub tick: usize,
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
            filter_active: false,
            filter_text: None,
            coverage_files: Vec::new(),
            coverage_selected: 0,
            coverage_sort: CoverageSort::PercentAsc,
            coverage_source_lines: Vec::new(),
            coverage_source_scroll: 0,
            coverage_threshold: 80.0,
            coverage_pending: false,
            coverage_drill_pending: false,
            coverage_tree_root: None,
            coverage_tree_selected: 0,
            rerun_pending: false,
            status_message: String::new(),
            shared_output: Arc::new(Mutex::new(Vec::new())),
            shared_results: Arc::new(Mutex::new(Vec::new())),
            tick: 0,
        }
    }

    /// Returns a flat list of `(depth, &TreeNode)` for all visible (expanded) nodes.
    /// When a filter is active, only nodes whose name matches the filter are returned.
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
    /// Returns `true` if a matching node was found.
    pub fn apply_test_result(&mut self, result: &TestResult) -> bool {
        // Convert PHP class to a path suffix for precise file matching.
        // e.g. `Tests\Feature\Auth\LoginTest` → `tests/Feature/Auth/LoginTest.php`
        let file_suffix = result.class.as_ref().map(|class| {
            let mut segments: Vec<&str> = class.split('\\').collect();
            // Find "Tests" segment and lowercase it to match filesystem convention
            if let Some(pos) = segments.iter().position(|s| s.eq_ignore_ascii_case("Tests")) {
                segments[pos] = "tests";
            }
            // Drop any prefix segments before "tests" (e.g. "P")
            let tests_pos = segments.iter().position(|s| *s == "tests").unwrap_or(0);
            let relevant = &segments[tests_pos..];
            let mut path = relevant.join("/");
            path.push_str(".php");
            path
        });

        Self::apply_result_to_node(&mut self.tree, result, file_suffix.as_deref())
    }

    fn apply_result_to_node(
        node: &mut TreeNode,
        result: &TestResult,
        file_suffix: Option<&str>,
    ) -> bool {
        if node.kind == NodeKind::Test && names_match(&node.name, &result.name) {
            // If we have a file suffix, verify the node's path matches
            let path_matches = match file_suffix {
                Some(suffix) => {
                    let node_path = node.path.to_string_lossy();
                    // Use forward-slash normalized path for comparison
                    let normalized = node_path.replace('\\', "/");
                    normalized.ends_with(suffix)
                }
                None => true, // No class info, accept any name match
            };
            if path_matches {
                node.status = result.status.clone();
                return true;
            }
        }
        for child in &mut node.children {
            if Self::apply_result_to_node(child, result, file_suffix) {
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

    /// Load coverage data from the `.pesticide/coverage.xml` Clover report.
    pub fn load_coverage(&mut self) -> Result<(), anyhow::Error> {
        let xml_path = self.project_root.join(".pesticide/coverage.xml");
        let xml = std::fs::read_to_string(&xml_path)
            .with_context(|| format!("Failed to read coverage file: {}", xml_path.display()))?;
        self.coverage_files = coverage::parse_clover_xml(&xml)?;
        self.sort_coverage();
        Ok(())
    }

    /// Build a coverage tree from the flat list of coverage files.
    /// Splits each file path on `/` to create a directory hierarchy,
    /// then aggregates stats (lines, hits, misses) upward to parent nodes.
    pub fn build_coverage_tree(&mut self) {
        use std::collections::BTreeMap;

        // Recursive struct to build the tree
        struct DirBuilder {
            children_dirs: BTreeMap<String, DirBuilder>,
            files: Vec<(String, usize, usize, usize, f64, String)>, // (name, lines, hits, misses, pct, full_path)
        }

        impl DirBuilder {
            fn new() -> Self {
                Self {
                    children_dirs: BTreeMap::new(),
                    files: Vec::new(),
                }
            }

            fn insert(&mut self, parts: &[&str], file: &CoverageFile) {
                if parts.len() == 1 {
                    self.files.push((
                        parts[0].to_string(),
                        file.lines,
                        file.hits,
                        file.misses,
                        file.percent,
                        file.path.clone(),
                    ));
                } else {
                    let dir = self
                        .children_dirs
                        .entry(parts[0].to_string())
                        .or_insert_with(DirBuilder::new);
                    dir.insert(&parts[1..], file);
                }
            }

            fn to_node(self, name: String, path_prefix: String) -> CoverageTreeNode {
                let mut children = Vec::new();

                // Add directory children first (sorted by name via BTreeMap)
                for (dir_name, builder) in self.children_dirs {
                    let child_path = if path_prefix.is_empty() {
                        dir_name.clone()
                    } else {
                        format!("{}/{}", path_prefix, dir_name)
                    };
                    children.push(builder.to_node(dir_name, child_path));
                }

                // Add file children (sorted by name)
                let mut files = self.files;
                files.sort_by(|a, b| a.0.cmp(&b.0));
                for (fname, lines, hits, misses, pct, full_path) in files {
                    children.push(CoverageTreeNode {
                        name: fname,
                        path: full_path,
                        is_file: true,
                        expanded: false,
                        children: Vec::new(),
                        lines,
                        hits,
                        misses,
                        percent: pct,
                    });
                }

                // Aggregate stats from children
                let total_lines: usize = children.iter().map(|c| c.lines).sum();
                let total_hits: usize = children.iter().map(|c| c.hits).sum();
                let total_misses: usize = children.iter().map(|c| c.misses).sum();
                let total_pct = if total_lines > 0 {
                    (total_hits as f64 / total_lines as f64) * 100.0
                } else {
                    0.0
                };

                CoverageTreeNode {
                    name,
                    path: path_prefix,
                    is_file: false,
                    expanded: true,
                    children,
                    lines: total_lines,
                    hits: total_hits,
                    misses: total_misses,
                    percent: total_pct,
                }
            }
        }

        let mut root_builder = DirBuilder::new();
        for file in &self.coverage_files {
            let parts: Vec<&str> = file.path.split('/').collect();
            root_builder.insert(&parts, file);
        }

        self.coverage_tree_root = Some(root_builder.to_node("Coverage".to_string(), String::new()));
        self.coverage_tree_selected = 0;
    }

    /// Returns a flat list of visible coverage tree nodes with depth.
    pub fn visible_coverage_tree_nodes(&self) -> Vec<(usize, &CoverageTreeNode)> {
        match &self.coverage_tree_root {
            Some(root) => root.flatten(0),
            None => Vec::new(),
        }
    }

    /// Get the currently selected coverage tree node's file path (if it's a file).
    pub fn selected_coverage_tree_file(&self) -> Option<&CoverageFile> {
        let nodes = self.visible_coverage_tree_nodes();
        if let Some((_, node)) = nodes.get(self.coverage_tree_selected) {
            if node.is_file {
                return self.coverage_files.iter().find(|f| f.path == node.path);
            }
        }
        None
    }

    /// Toggle expand/collapse on the selected coverage tree node.
    pub fn toggle_coverage_tree_expand(&mut self) {
        let nodes = self.visible_coverage_tree_nodes();
        if let Some((_, node)) = nodes.get(self.coverage_tree_selected) {
            if !node.is_file {
                let path = node.path.clone();
                if let Some(ref mut root) = self.coverage_tree_root {
                    Self::toggle_coverage_node(root, &path);
                }
            }
        }
    }

    fn toggle_coverage_node(node: &mut CoverageTreeNode, path: &str) -> bool {
        if node.path == path && !node.is_file {
            node.expanded = !node.expanded;
            return true;
        }
        for child in &mut node.children {
            if Self::toggle_coverage_node(child, path) {
                return true;
            }
        }
        false
    }

    /// Load source-level coverage for the currently selected coverage file.
    pub fn load_coverage_source(&mut self) -> Result<(), anyhow::Error> {
        if self.coverage_selected >= self.coverage_files.len() {
            return Ok(());
        }
        let file_path = self.coverage_files[self.coverage_selected].path.clone();

        let xml_path = self.project_root.join(".pesticide/coverage.xml");
        let xml = std::fs::read_to_string(&xml_path)
            .with_context(|| format!("Failed to read coverage file: {}", xml_path.display()))?;

        let line_hits = coverage::parse_file_line_coverage(&xml, &file_path)?;
        let source_path = Path::new(&file_path);
        // If the path in the XML is relative, resolve it against the project root
        let resolved_path = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            self.project_root.join(source_path)
        };
        self.coverage_source_lines = coverage::build_coverage_source(&resolved_path, &line_hits)?;
        self.coverage_source_scroll = 0;
        Ok(())
    }
}

/// Applies the same lossy encoding Pest uses when converting test descriptions
/// to PHP method names. This maps multiple characters to the same representation,
/// making both sides comparable.
///
/// Pest's encoding: space→`_`, hyphen→`_`, comma+space→`__`, backtick→`__` boundary
/// Discovery reversal: `__`→`_`, `_`→space
///
/// So if we apply the same lossy encoding to the JUnit name (which preserves
/// original characters), it should match the discovery-derived tree name.
fn lossy_normalize(name: &str) -> String {
    let s = name.trim();
    // Strip trailing period (JUnit may strip it, tree may keep it)
    let s = s.strip_suffix('.').unwrap_or(s);
    s
        // Strip backticks (Pest encodes them as `__` boundaries)
        .replace('`', "")
        // PHP `::` → underscore (Pest encodes as `__`, discovery reverses to `_`)
        .replace("::", "_")
        // Comma+space → underscore (Pest encodes as `__`, discovery reverses to `_`)
        .replace(", ", "_")
        // Hyphens → space (Pest encodes as `_`, discovery reverses to space)
        .replace('-', " ")
        // Normalize arrow separator: `_→` and ` → ` to a single canonical form
        .replace("_→", " →")
        // Collapse multiple spaces
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Flexible name comparison for matching JUnit test results to tree nodes.
///
/// Tries multiple strategies:
/// 1. Exact match
/// 2. Lossy normalization (apply Pest's encoding to both sides)
/// 3. Normalize internal `__pest_evaluable_` names
fn names_match(tree_name: &str, result_name: &str) -> bool {
    let tree_trimmed = tree_name.trim();
    let result_trimmed = result_name.trim();

    // Exact match
    if tree_trimmed == result_trimmed {
        return true;
    }

    // Lossy normalization: apply Pest's encoding to the JUnit name so it
    // matches the discovery-derived tree name
    if lossy_normalize(tree_trimmed) == lossy_normalize(result_trimmed) {
        return true;
    }

    // Normalize result name the same way discovery does (in case JUnit reports internal names)
    let normalized = result_trimmed
        .strip_prefix("__pest_evaluable_")
        .unwrap_or(result_trimmed)
        .replace("__", "\x00")
        .replace('_', " ")
        .replace('\x00', "_");

    if tree_trimmed == normalized {
        return true;
    }

    // Case-insensitive fallback
    if tree_trimmed.eq_ignore_ascii_case(result_trimmed) {
        return true;
    }

    false
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
        assert!(!app.filter_active);
        assert!(app.filter_text.is_none());
        assert!(app.coverage_files.is_empty());
        assert_eq!(app.coverage_selected, 0);
        assert_eq!(app.coverage_sort, CoverageSort::PercentAsc);
        assert!(app.coverage_source_lines.is_empty());
        assert_eq!(app.coverage_source_scroll, 0);
        assert_eq!(app.coverage_threshold, 80.0);
        assert!(!app.coverage_pending);
        assert!(!app.coverage_drill_pending);
        assert!(app.coverage_tree_root.is_none());
        assert_eq!(app.coverage_tree_selected, 0);
        assert!(app.status_message.is_empty());
    }

    #[test]
    fn test_names_match_exact() {
        assert!(names_match("it can login", "it can login"));
    }

    #[test]
    fn test_names_match_trimmed() {
        assert!(names_match("it can login", "it can login "));
        assert!(names_match("it can login ", "it can login"));
    }

    #[test]
    fn test_names_match_pest_evaluable_prefix() {
        assert!(names_match(
            "it can login",
            "__pest_evaluable_it_can_login"
        ));
    }

    #[test]
    fn test_names_match_pest_evaluable_with_double_underscores() {
        assert!(names_match(
            "it sets check_card when igt_id changes",
            "__pest_evaluable_it_sets_check__card_when_igt__id_changes"
        ));
    }

    #[test]
    fn test_names_match_case_insensitive() {
        assert!(names_match("It Can Login", "it can login"));
    }

    #[test]
    fn test_names_match_hyphens() {
        // Hyphens in JUnit become spaces in tree (Pest encodes both as _)
        assert!(names_match(
            "it blocks non members from viewing injuries",
            "it blocks non-members from viewing injuries"
        ));
    }

    #[test]
    fn test_names_match_commas() {
        // Commas in JUnit become underscores in tree (Pest encodes ", " as __)
        assert!(names_match(
            "it allows editors to create_update_and delete games",
            "it allows editors to create, update, and delete games"
        ));
    }

    #[test]
    fn test_names_match_backticks() {
        // Backtick-wrapped method names in describe() blocks
        assert!(names_match(
            "calculateGroupStats → it handles single value",
            "`calculateGroupStats` → it handles single value"
        ));
    }

    #[test]
    fn test_names_match_backtick_arrow_underscore() {
        // Tree has `_→` (underscore before arrow), JUnit has ` → ` (space before arrow)
        assert!(names_match(
            "calculateMedian_→ it returns 0 for empty",
            "`calculateMedian` → it returns 0 for empty"
        ));
    }

    #[test]
    fn test_names_match_double_colon() {
        // PHP :: in test name becomes _ in tree
        assert!(names_match(
            "it defines deidentified_viewer in Roles_ALL",
            "it defines deidentified_viewer in Roles::ALL"
        ));
    }

    #[test]
    fn test_names_match_trailing_period() {
        // Tree may have trailing period that JUnit strips
        assert!(names_match(
            "it returns empty cost distribution when all costs are zero.",
            "it returns empty cost distribution when all costs are zero"
        ));
    }

    #[test]
    fn test_names_match_no_match() {
        assert!(!names_match("it can login", "it can register"));
    }

    #[test]
    fn test_lossy_normalize_hyphens() {
        assert_eq!(lossy_normalize("non-members"), "non members");
    }

    #[test]
    fn test_lossy_normalize_commas() {
        assert_eq!(lossy_normalize("create, update, and"), "create_update_and");
    }

    #[test]
    fn test_lossy_normalize_backticks() {
        assert_eq!(
            lossy_normalize("`calculateGroupStats` → it works"),
            "calculateGroupStats → it works"
        );
    }

    #[test]
    fn test_lossy_normalize_arrow_separator() {
        // Both `_→` and ` → ` should normalize to the same thing
        assert_eq!(
            lossy_normalize("calculateMedian_→ it returns 0"),
            lossy_normalize("`calculateMedian` → it returns 0")
        );
    }

    #[test]
    fn test_lossy_normalize_double_colon() {
        assert_eq!(lossy_normalize("Roles::ALL"), "Roles_ALL");
    }

    #[test]
    fn test_lossy_normalize_trailing_period() {
        assert_eq!(lossy_normalize("costs are zero."), "costs are zero");
    }

    #[test]
    fn test_apply_test_result_returns_true_on_match() {
        use crate::tree::node::TestStatus;

        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        let result = TestResult {
            name: "it works".to_string(),
            status: TestStatus::Passed,
            class: None,
        };
        assert!(app.apply_test_result(&result));
    }

    #[test]
    fn test_apply_test_result_returns_false_on_no_match() {
        use crate::tree::node::TestStatus;

        let tree = make_test_tree();
        let mut app = App::new(tree, PathBuf::from("/project"));
        let result = TestResult {
            name: "nonexistent test".to_string(),
            status: TestStatus::Passed,
            class: None,
        };
        assert!(!app.apply_test_result(&result));
    }

    #[test]
    fn test_apply_test_result_with_class_matches_correct_file() {
        use crate::tree::node::TestStatus;

        // Build a tree with two files that both have a test named "it works"
        let mut root = TreeNode::new_root(PathBuf::from("/project"));
        let mut dir = TreeNode::new_directory(
            "Feature".to_string(),
            PathBuf::from("/project/tests/Feature"),
        );
        dir.expanded = true;

        let mut file_a = TreeNode::new_file(
            "AlphaTest.php".to_string(),
            PathBuf::from("/project/tests/Feature/AlphaTest.php"),
        );
        file_a.expanded = true;
        file_a.children.push(TreeNode::new_test(
            "it works".to_string(),
            PathBuf::from("/project/tests/Feature/AlphaTest.php"),
        ));

        let mut file_b = TreeNode::new_file(
            "BetaTest.php".to_string(),
            PathBuf::from("/project/tests/Feature/BetaTest.php"),
        );
        file_b.expanded = true;
        file_b.children.push(TreeNode::new_test(
            "it works".to_string(),
            PathBuf::from("/project/tests/Feature/BetaTest.php"),
        ));

        dir.children.push(file_a);
        dir.children.push(file_b);
        root.children.push(dir);

        let mut app = App::new(root, PathBuf::from("/project"));

        // Apply result for BetaTest — should only update the Beta file's test
        let result = TestResult {
            name: "it works".to_string(),
            status: TestStatus::Passed,
            class: Some("Tests\\Feature\\BetaTest".to_string()),
        };
        assert!(app.apply_test_result(&result));

        // AlphaTest's "it works" should still be NotRun
        let alpha_test = &app.tree.children[0].children[0].children[0];
        assert_eq!(alpha_test.status, TestStatus::NotRun);

        // BetaTest's "it works" should be Passed
        let beta_test = &app.tree.children[0].children[1].children[0];
        assert_eq!(beta_test.status, TestStatus::Passed);
    }
}
