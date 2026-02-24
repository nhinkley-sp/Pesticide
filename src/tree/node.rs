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
    /// Creates a new root node (expanded by default).
    pub fn new_root(path: PathBuf) -> Self {
        Self {
            name: String::from("Root"),
            kind: NodeKind::Root,
            path,
            status: TestStatus::NotRun,
            expanded: true,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 0,
        }
    }

    /// Creates a new directory node (collapsed by default).
    pub fn new_directory(name: String, path: PathBuf) -> Self {
        Self {
            name,
            kind: NodeKind::Directory,
            path,
            status: TestStatus::NotRun,
            expanded: false,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 0,
        }
    }

    /// Creates a new file node (collapsed by default).
    pub fn new_file(name: String, path: PathBuf) -> Self {
        Self {
            name,
            kind: NodeKind::File,
            path,
            status: TestStatus::NotRun,
            expanded: false,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 0,
        }
    }

    /// Creates a new test node (test_count = 1, leaf node).
    pub fn new_test(name: String, path: PathBuf) -> Self {
        Self {
            name,
            kind: NodeKind::Test,
            path,
            status: TestStatus::NotRun,
            expanded: false,
            children: Vec::new(),
            coverage_percent: None,
            test_count: 1,
        }
    }

    /// Adds a child node and updates this node's test_count.
    pub fn add_child(&mut self, child: TreeNode) {
        self.test_count += child.test_count;
        self.children.push(child);
    }

    /// Returns a flat list of `(depth, &TreeNode)` for all visible (expanded) nodes.
    ///
    /// The root node itself is included at depth 0. Children of collapsed nodes
    /// are not included.
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

    /// Recalculates test_count from children recursively.
    ///
    /// Leaf test nodes keep their count of 1. Parent nodes sum their children's counts.
    pub fn recalculate_counts(&mut self) {
        if self.kind == NodeKind::Test {
            self.test_count = 1;
            return;
        }
        for child in &mut self.children {
            child.recalculate_counts();
        }
        self.test_count = self.children.iter().map(|c| c.test_count).sum();
    }

    /// Aggregates status from children:
    /// - `Failed` if any child failed
    /// - `Running` if any child is running (and none failed)
    /// - `Passed` if all children passed
    /// - `NotRun` otherwise (including empty nodes)
    pub fn aggregate_status(&self) -> TestStatus {
        if self.children.is_empty() {
            return self.status.clone();
        }

        let mut any_failed = false;
        let mut any_running = false;
        let mut all_passed = true;

        for child in &self.children {
            let child_status = if child.children.is_empty() {
                child.status.clone()
            } else {
                child.aggregate_status()
            };

            match child_status {
                TestStatus::Failed => {
                    any_failed = true;
                    all_passed = false;
                }
                TestStatus::Running => {
                    any_running = true;
                    all_passed = false;
                }
                TestStatus::Passed => {}
                TestStatus::NotRun => {
                    all_passed = false;
                }
            }
        }

        if any_failed {
            TestStatus::Failed
        } else if any_running {
            TestStatus::Running
        } else if all_passed && !self.children.is_empty() {
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
        let dir = TreeNode::new_directory(
            "Feature".to_string(),
            PathBuf::from("tests/Feature"),
        );
        assert_eq!(dir.name, "Feature");
        assert_eq!(dir.kind, NodeKind::Directory);
        assert!(!dir.expanded);
        assert_eq!(dir.test_count, 0);
        assert!(dir.children.is_empty());
        assert_eq!(dir.status, TestStatus::NotRun);
    }

    #[test]
    fn test_add_child_updates_count() {
        let mut file = TreeNode::new_file(
            "ExampleTest.php".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        assert_eq!(file.test_count, 0);

        let test1 = TreeNode::new_test(
            "it can do something".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        file.add_child(test1);
        assert_eq!(file.test_count, 1);

        let test2 = TreeNode::new_test(
            "it can do another thing".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        file.add_child(test2);
        assert_eq!(file.test_count, 2);
    }

    #[test]
    fn test_flatten_collapsed() {
        // Root is expanded, child directory is collapsed.
        // We should see root + child = 2 items.
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut dir = TreeNode::new_directory(
            "Feature".to_string(),
            PathBuf::from("tests/Feature"),
        );
        let test = TreeNode::new_test(
            "it works".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        dir.add_child(test);
        // dir is collapsed by default
        root.add_child(dir);

        let flat = root.flatten();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].0, 0); // root at depth 0
        assert_eq!(flat[0].1.kind, NodeKind::Root);
        assert_eq!(flat[1].0, 1); // directory at depth 1
        assert_eq!(flat[1].1.kind, NodeKind::Directory);
    }

    #[test]
    fn test_flatten_expanded() {
        // Root expanded, child directory expanded → we see root + dir + grandchild = 3 items.
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut dir = TreeNode::new_directory(
            "Feature".to_string(),
            PathBuf::from("tests/Feature"),
        );
        dir.expanded = true; // expand the directory
        let test = TreeNode::new_test(
            "it works".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        dir.add_child(test);
        root.add_child(dir);

        let flat = root.flatten();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].0, 0); // root at depth 0
        assert_eq!(flat[0].1.kind, NodeKind::Root);
        assert_eq!(flat[1].0, 1); // directory at depth 1
        assert_eq!(flat[1].1.kind, NodeKind::Directory);
        assert_eq!(flat[2].0, 2); // test at depth 2
        assert_eq!(flat[2].1.kind, NodeKind::Test);
    }

    #[test]
    fn test_aggregate_status_all_passed() {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut test1 = TreeNode::new_test(
            "test one".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test1.status = TestStatus::Passed;
        let mut test2 = TreeNode::new_test(
            "test two".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test2.status = TestStatus::Passed;
        root.add_child(test1);
        root.add_child(test2);

        assert_eq!(root.aggregate_status(), TestStatus::Passed);
    }

    #[test]
    fn test_aggregate_status_any_failed() {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut test1 = TreeNode::new_test(
            "test one".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test1.status = TestStatus::Passed;
        let mut test2 = TreeNode::new_test(
            "test two".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test2.status = TestStatus::Failed;
        root.add_child(test1);
        root.add_child(test2);

        assert_eq!(root.aggregate_status(), TestStatus::Failed);
    }

    #[test]
    fn test_aggregate_status_not_run() {
        // An empty directory should report NotRun.
        let dir = TreeNode::new_directory(
            "Empty".to_string(),
            PathBuf::from("tests/Empty"),
        );
        assert_eq!(dir.aggregate_status(), TestStatus::NotRun);
    }

    #[test]
    fn test_aggregate_status_running() {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut test1 = TreeNode::new_test(
            "test one".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test1.status = TestStatus::Running;
        let mut test2 = TreeNode::new_test(
            "test two".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test2.status = TestStatus::Passed;
        root.add_child(test1);
        root.add_child(test2);

        assert_eq!(root.aggregate_status(), TestStatus::Running);
    }

    #[test]
    fn test_recalculate_counts() {
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut dir = TreeNode::new_directory(
            "Feature".to_string(),
            PathBuf::from("tests/Feature"),
        );
        let test1 = TreeNode::new_test(
            "test one".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        let test2 = TreeNode::new_test(
            "test two".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        dir.children.push(test1);
        dir.children.push(test2);
        // Counts are wrong because we bypassed add_child
        root.children.push(dir);

        assert_eq!(root.test_count, 0);
        root.recalculate_counts();
        assert_eq!(root.test_count, 2);
        assert_eq!(root.children[0].test_count, 2);
    }

    #[test]
    fn test_new_root_is_expanded() {
        let root = TreeNode::new_root(PathBuf::from("tests"));
        assert!(root.expanded);
        assert_eq!(root.kind, NodeKind::Root);
        assert_eq!(root.test_count, 0);
    }

    #[test]
    fn test_new_test_has_count_one() {
        let test = TreeNode::new_test(
            "it works".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        assert_eq!(test.test_count, 1);
        assert_eq!(test.kind, NodeKind::Test);
    }

    #[test]
    fn test_aggregate_status_nested() {
        // Test that aggregate_status works through nested directories.
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut dir = TreeNode::new_directory(
            "Feature".to_string(),
            PathBuf::from("tests/Feature"),
        );
        let mut test1 = TreeNode::new_test(
            "test one".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        test1.status = TestStatus::Passed;
        let mut test2 = TreeNode::new_test(
            "test two".to_string(),
            PathBuf::from("tests/Feature/ExampleTest.php"),
        );
        test2.status = TestStatus::Failed;
        dir.add_child(test1);
        dir.add_child(test2);
        root.add_child(dir);

        // Root should aggregate through the directory to find the failure.
        assert_eq!(root.aggregate_status(), TestStatus::Failed);
    }

    #[test]
    fn test_failed_overrides_running() {
        // Failed should take priority over Running.
        let mut root = TreeNode::new_root(PathBuf::from("tests"));
        let mut test1 = TreeNode::new_test(
            "test one".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test1.status = TestStatus::Running;
        let mut test2 = TreeNode::new_test(
            "test two".to_string(),
            PathBuf::from("tests/ExampleTest.php"),
        );
        test2.status = TestStatus::Failed;
        root.add_child(test1);
        root.add_child(test2);

        assert_eq!(root.aggregate_status(), TestStatus::Failed);
    }
}
