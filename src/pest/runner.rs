use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

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
    /// Kills the child process.
    pub fn kill(&mut self) {
        // start_kill sends the kill signal without waiting
        let _ = self.child.start_kill();
    }
}

/// Builds a `tokio::process::Command` for running Pest tests.
///
/// - Uses `vendor/bin/pest` from `project_root`
/// - Sets `current_dir` to `project_root`
/// - Adds path/filter args based on `scope`
/// - Adds `--parallel` if `parallel` is true
/// - Always adds `--teamcity` for structured output parsing
/// - If `coverage` is true, adds `--coverage-clover .pesticide/coverage.xml`
/// - Sets stdout/stderr to piped
pub fn build_pest_command(
    project_root: &Path,
    scope: &RunScope,
    parallel: bool,
    coverage: bool,
) -> Command {
    let pest_bin = project_root.join("vendor/bin/pest");
    let mut cmd = Command::new(pest_bin);
    cmd.current_dir(project_root);

    // Add scope-specific arguments
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

    // Always add --teamcity for structured output parsing
    cmd.arg("--teamcity");

    if coverage {
        cmd.arg("--coverage-clover");
        cmd.arg(".pesticide/coverage.xml");
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    cmd
}

/// Spawns the Pest test runner and streams output line by line.
///
/// - Creates `.pesticide` dir if needed
/// - Builds and spawns the command
/// - Spawns tokio tasks to stream stdout/stderr line by line
/// - Parses TeamCity messages from stdout to build `TestResult`s
/// - Pushes all lines to `output_lines`
/// - Returns a `RunHandle` holding the child process
pub fn run_tests(
    project_root: &Path,
    scope: &RunScope,
    parallel: bool,
    coverage: bool,
    output_lines: Arc<Mutex<Vec<String>>>,
    results: Arc<Mutex<Vec<TestResult>>>,
) -> Result<RunHandle> {
    // Create .pesticide directory if it doesn't exist
    let pesticide_dir = project_root.join(".pesticide");
    std::fs::create_dir_all(&pesticide_dir)?;

    let mut cmd = build_pest_command(project_root, scope, parallel, coverage);
    let mut child = cmd.spawn()?;

    // Take stdout and stderr handles
    let stdout = child.stdout.take().expect("stdout should be piped");
    let stderr = child.stderr.take().expect("stderr should be piped");

    // Spawn task for stdout
    let stdout_output_lines = Arc::clone(&output_lines);
    let stdout_results = Arc::clone(&results);
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // Parse TeamCity messages for test results
            if line.contains("##teamcity[testFailed") {
                if let Some(name) = extract_teamcity_attr(&line, "name") {
                    let mut res = stdout_results.lock().unwrap();
                    res.push(TestResult {
                        name,
                        status: TestStatus::Failed,
                    });
                }
            } else if line.contains("##teamcity[testFinished") {
                if let Some(name) = extract_teamcity_attr(&line, "name") {
                    // Only mark as passed if not already marked as failed
                    let mut res = stdout_results.lock().unwrap();
                    let already_failed = res.iter().any(|r| {
                        r.name == name && r.status == TestStatus::Failed
                    });
                    if !already_failed {
                        res.push(TestResult {
                            name,
                            status: TestStatus::Passed,
                        });
                    }
                }
            }

            let mut lines_vec = stdout_output_lines.lock().unwrap();
            lines_vec.push(line);
        }
    });

    // Spawn task for stderr
    let stderr_output_lines = Arc::clone(&output_lines);
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut lines_vec = stderr_output_lines.lock().unwrap();
            lines_vec.push(line);
        }
    });

    Ok(RunHandle { child })
}

/// Extracts an attribute value from a TeamCity message line.
///
/// TeamCity format: `##teamcity[messageName attr1='value1' attr2='value2']`
///
/// Handles TeamCity escaping:
/// - `|'` for `'`
/// - `|n` for newline
/// - `||` for `|`
/// - `|[` for `[`
/// - `|]` for `]`
pub fn extract_teamcity_attr(line: &str, attr: &str) -> Option<String> {
    // Look for attr='...' pattern
    let search = format!("{}='", attr);
    let start_idx = line.find(&search)?;
    let value_start = start_idx + search.len();

    let remaining = &line[value_start..];

    // Find the closing unescaped single quote
    let mut result = String::new();
    let mut chars = remaining.chars();
    loop {
        match chars.next() {
            None => return None, // unterminated value
            Some('|') => {
                // Escape sequence
                match chars.next() {
                    Some('\'') => result.push('\''),
                    Some('n') => result.push('\n'),
                    Some('|') => result.push('|'),
                    Some('[') => result.push('['),
                    Some(']') => result.push(']'),
                    Some(c) => {
                        // Unknown escape, preserve both characters
                        result.push('|');
                        result.push(c);
                    }
                    None => return None,
                }
            }
            Some('\'') => {
                // End of value
                return Some(result);
            }
            Some(c) => result.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_teamcity_attr() {
        let line = "##teamcity[testFinished name='it can login' duration='42']";
        assert_eq!(
            extract_teamcity_attr(line, "name"),
            Some("it can login".to_string())
        );
        assert_eq!(
            extract_teamcity_attr(line, "duration"),
            Some("42".to_string())
        );
    }

    #[test]
    fn test_extract_teamcity_attr_escaped() {
        let line = "##teamcity[testFailed name='it|'s a test' message='fail']";
        assert_eq!(
            extract_teamcity_attr(line, "name"),
            Some("it's a test".to_string())
        );
    }

    #[test]
    fn test_extract_teamcity_attr_missing() {
        let line = "##teamcity[testFinished name='test']";
        assert_eq!(extract_teamcity_attr(line, "missing"), None);
    }

    #[test]
    fn test_build_command_all_parallel() {
        let cmd = build_pest_command(Path::new("/project"), &RunScope::All, true, false);
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();
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
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();
        assert!(args.contains(&"tests/Feature/LoginTest.php".to_string()));
        assert!(!args.contains(&"--parallel".to_string()));
    }

    #[test]
    fn test_build_command_with_coverage() {
        let cmd = build_pest_command(Path::new("/project"), &RunScope::All, true, true);
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();
        assert!(args.contains(&"--coverage-clover".to_string()));
    }

    #[test]
    fn test_build_command_single_test() {
        let cmd = build_pest_command(
            Path::new("/project"),
            &RunScope::Test {
                file: PathBuf::from("tests/ExampleTest.php"),
                name: "it works".to_string(),
            },
            true,
            false,
        );
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();
        assert!(args.contains(&"tests/ExampleTest.php".to_string()));
        assert!(args.contains(&"--filter".to_string()));
        assert!(args.contains(&"it works".to_string()));
    }
}
