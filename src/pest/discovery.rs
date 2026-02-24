use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::tree::node::{NodeKind, TreeNode};

/// Parses the output of `pest --list-tests` into a tree structure.
///
/// Each line is expected to be in the format:
///   `- Namespace\Class::test_name`
///
/// The namespace is converted to a directory/file hierarchy:
///   `Tests\Feature\Auth\LoginTest::it_can_login` becomes
///   Feature/ -> Auth/ -> LoginTest.php -> "it can login"
///
/// The leading "Tests" segment is skipped.
pub fn parse_test_list(output: &str, project_root: &Path) -> TreeNode {
    let mut root = TreeNode::new_root(project_root.to_path_buf());

    for line in output.lines() {
        let line = line.trim();

        // Each valid line starts with "- "
        let entry = match line.strip_prefix("- ") {
            Some(entry) => entry,
            None => continue,
        };

        // Split on "::" to get the class path and test name
        let (class_part, test_name) = match entry.split_once("::") {
            Some((cls, name)) => (cls, name),
            None => continue,
        };

        // Split namespace segments on backslash
        let segments: Vec<&str> = class_part.split('\\').collect();

        // We need at least 2 segments (e.g. "Tests\SomeTest") to have
        // a "Tests" prefix to skip plus a class name.
        if segments.len() < 2 {
            continue;
        }

        // Skip the leading "Tests" segment
        let segments = &segments[1..];

        // The last segment is the file (class) name, everything before is directories
        let (dir_segments, file_segment) = segments.split_at(segments.len() - 1);
        let file_name = format!("{}.php", file_segment[0]);
        let display_name = test_name.replace('_', " ");

        // Build the path incrementally for node paths
        let mut current_path = project_root.to_path_buf();

        // Navigate/create directory nodes
        let mut current_node = &mut root;
        for &dir_name in dir_segments {
            current_path = current_path.join(dir_name);

            // Find or create the directory child
            let pos = current_node
                .children
                .iter()
                .position(|c| c.kind == NodeKind::Directory && c.name == dir_name);

            let idx = match pos {
                Some(idx) => idx,
                None => {
                    let dir_node =
                        TreeNode::new_directory(dir_name.to_string(), current_path.clone());
                    current_node.children.push(dir_node);
                    current_node.children.len() - 1
                }
            };
            current_node = &mut current_node.children[idx];
        }

        // Navigate/create the file node
        let file_path = current_path.join(&file_name);
        let file_pos = current_node
            .children
            .iter()
            .position(|c| c.kind == NodeKind::File && c.name == file_name);

        let file_idx = match file_pos {
            Some(idx) => idx,
            None => {
                let file_node = TreeNode::new_file(file_name.clone(), file_path.clone());
                current_node.children.push(file_node);
                current_node.children.len() - 1
            }
        };
        let file_node = &mut current_node.children[file_idx];

        // Add the test node
        let test_node = TreeNode::new_test(display_name, file_path.clone());
        file_node.children.push(test_node);
    }

    root.recalculate_counts();
    root
}

/// Walks upward from `start` looking for a directory containing `vendor/bin/pest`.
///
/// Returns `Some(path)` with the project root if found, `None` otherwise.
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

/// Runs `vendor/bin/pest --list-tests` in the given project root and returns stdout.
pub fn run_list_tests(project_root: &Path) -> Result<String> {
    let pest_bin = project_root.join("vendor/bin/pest");
    let output = Command::new(&pest_bin)
        .arg("--list-tests")
        .current_dir(project_root)
        .output()
        .with_context(|| format!("Failed to run {}", pest_bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "pest --list-tests exited with status {}: {}",
            output.status,
            stderr.trim()
        );
    }

    String::from_utf8(output.stdout).context("pest --list-tests output was not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SAMPLE_OUTPUT: &str = "\
- Tests\\Feature\\Auth\\LoginTest::it_can_login_with_valid_credentials
- Tests\\Feature\\Auth\\LoginTest::it_rejects_invalid_password
- Tests\\Feature\\Auth\\RegisterTest::it_can_register_new_user
- Tests\\Unit\\Models\\PlayerTest::it_returns_false_when_player_has_no_injuries
- Tests\\Unit\\Models\\PlayerTest::it_returns_true_when_player_has_active_injury
";

    #[test]
    fn test_parse_creates_root() {
        let root = parse_test_list(SAMPLE_OUTPUT, &PathBuf::from("/project"));
        // Root should have 2 children: Feature and Unit
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].name, "Feature");
        assert_eq!(root.children[1].name, "Unit");
    }

    #[test]
    fn test_parse_directory_structure() {
        let root = parse_test_list(SAMPLE_OUTPUT, &PathBuf::from("/project"));
        // Feature > Auth > 2 files (LoginTest.php, RegisterTest.php)
        let feature = &root.children[0];
        assert_eq!(feature.kind, NodeKind::Directory);
        assert_eq!(feature.children.len(), 1); // Auth
        let auth = &feature.children[0];
        assert_eq!(auth.name, "Auth");
        assert_eq!(auth.kind, NodeKind::Directory);
        assert_eq!(auth.children.len(), 2); // LoginTest.php, RegisterTest.php
    }

    #[test]
    fn test_parse_test_names() {
        let root = parse_test_list(SAMPLE_OUTPUT, &PathBuf::from("/project"));
        let feature = &root.children[0];
        let auth = &feature.children[0];
        let login_test = &auth.children[0];
        assert_eq!(login_test.name, "LoginTest.php");
        assert_eq!(login_test.kind, NodeKind::File);
        assert_eq!(login_test.children.len(), 2);
        // Underscores should be replaced with spaces
        assert_eq!(
            login_test.children[0].name,
            "it can login with valid credentials"
        );
        assert_eq!(
            login_test.children[1].name,
            "it rejects invalid password"
        );
    }

    #[test]
    fn test_parse_counts() {
        let root = parse_test_list(SAMPLE_OUTPUT, &PathBuf::from("/project"));
        assert_eq!(root.test_count, 5);
        // Feature has 3 tests (2 in LoginTest + 1 in RegisterTest)
        assert_eq!(root.children[0].test_count, 3);
        // Unit has 2 tests
        assert_eq!(root.children[1].test_count, 2);
    }

    #[test]
    fn test_parse_empty_output() {
        let root = parse_test_list("", &PathBuf::from("/project"));
        assert_eq!(root.children.len(), 0);
        assert_eq!(root.test_count, 0);
    }

    #[test]
    fn test_parse_malformed_lines() {
        let input = "\
not a valid line
- MissingDoubleColon
- Tests\\Feature\\SomeTest::valid_test
random garbage
";
        let root = parse_test_list(input, &PathBuf::from("/project"));
        // Only the one valid line should be parsed
        assert_eq!(root.test_count, 1);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "Feature");
    }

    #[test]
    fn test_find_project_root_returns_none_for_nonexistent() {
        // A path that almost certainly won't have vendor/bin/pest
        let result = find_project_root(Path::new("/tmp/nonexistent_pest_project_xyz"));
        assert!(result.is_none());
    }

    #[test]
    fn test_find_project_root_finds_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let vendor_bin = tmp.path().join("vendor/bin");
        std::fs::create_dir_all(&vendor_bin).unwrap();
        std::fs::write(vendor_bin.join("pest"), "#!/bin/bash").unwrap();

        let subdir = tmp.path().join("tests/Feature");
        std::fs::create_dir_all(&subdir).unwrap();

        let result = find_project_root(&subdir);
        assert_eq!(result, Some(tmp.path().to_path_buf()));
    }
}
