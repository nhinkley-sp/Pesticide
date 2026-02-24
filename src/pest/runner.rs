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
    /// PHP class name from JUnit, e.g. `Tests\Feature\Auth\LoginTest`
    pub class: Option<String>,
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

    // Add scope-specific arguments — Pest expects paths relative to project root
    match scope {
        RunScope::All => {}
        RunScope::File(path) => {
            let rel = path.strip_prefix(project_root).unwrap_or(path);
            cmd.arg(rel);
        }
        RunScope::Directory(path) => {
            let rel = path.strip_prefix(project_root).unwrap_or(path);
            cmd.arg(rel);
        }
        RunScope::Test { file, name } => {
            let rel = file.strip_prefix(project_root).unwrap_or(file);
            cmd.arg(rel);
            cmd.arg("--filter");
            cmd.arg(name);
        }
    }

    if parallel {
        cmd.arg("--parallel");
    }

    // Use --log-junit for structured result parsing (works with both parallel and sequential)
    let junit_path = project_root.join(".pesticide/results.xml");
    cmd.arg("--log-junit");
    cmd.arg(&junit_path);

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
/// - Pushes all lines to `output_lines`
/// - Returns a `RunHandle` holding the child process
///
/// Note: Test results are parsed from the JUnit XML file after the run completes
/// (see `parse_junit_results`), not from the streaming output.
pub fn run_tests(
    project_root: &Path,
    scope: &RunScope,
    parallel: bool,
    coverage: bool,
    output_lines: Arc<Mutex<Vec<String>>>,
    _results: Arc<Mutex<Vec<TestResult>>>,
) -> Result<RunHandle> {
    // Create .pesticide directory if it doesn't exist
    let pesticide_dir = project_root.join(".pesticide");
    std::fs::create_dir_all(&pesticide_dir)?;

    let mut cmd = build_pest_command(project_root, scope, parallel, coverage);
    let mut child = cmd.spawn()?;

    // Take stdout and stderr handles
    let stdout = child.stdout.take().expect("stdout should be piped");
    let stderr = child.stderr.take().expect("stderr should be piped");

    // Spawn task for stdout — stream output for display, strip ANSI codes
    let stdout_output_lines = Arc::clone(&output_lines);
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let clean = strip_ansi_codes(&line);
            if !clean.trim().is_empty() {
                let mut lines_vec = stdout_output_lines.lock().unwrap();
                lines_vec.push(clean);
            }
        }
    });

    // Spawn task for stderr — strip ANSI codes
    let stderr_output_lines = Arc::clone(&output_lines);
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let clean = strip_ansi_codes(&line);
            if !clean.trim().is_empty() {
                let mut lines_vec = stderr_output_lines.lock().unwrap();
                lines_vec.push(clean);
            }
        }
    });

    Ok(RunHandle { child })
}

/// Parse JUnit XML results file written by `--log-junit`.
/// Returns a list of TestResult with pass/fail status for each test.
pub fn parse_junit_results(project_root: &Path) -> Vec<TestResult> {
    let junit_path = project_root.join(".pesticide/results.xml");
    let xml = match std::fs::read_to_string(&junit_path) {
        Ok(xml) => xml,
        Err(_) => return Vec::new(),
    };

    let doc = match roxmltree::Document::parse(&xml) {
        Ok(doc) => doc,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for testcase in doc.descendants().filter(|n| n.has_tag_name("testcase")) {
        let name = match testcase.attribute("name") {
            Some(n) => n.to_string(),
            None => continue,
        };

        // A testcase with a <failure> child element is a failure
        let has_failure = testcase.children().any(|c| c.has_tag_name("failure"));
        let has_error = testcase.children().any(|c| c.has_tag_name("error"));
        let has_skipped = testcase.children().any(|c| c.has_tag_name("skipped"));

        let status = if has_failure || has_error {
            TestStatus::Failed
        } else if has_skipped {
            TestStatus::NotRun
        } else {
            TestStatus::Passed
        };

        let class = testcase
            .attribute("class")
            .or_else(|| testcase.attribute("classname"))
            .map(|s| s.to_string());

        results.push(TestResult { name, status, class });
    }

    results
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC sequence — skip until we hit a letter or reach end
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: ESC [ ... <letter>
                    chars.next(); // consume '['
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: ESC ] ... ST (or BEL)
                    chars.next(); // consume ']'
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next == '\x07' || next == '\\' {
                            break;
                        }
                    }
                }
                _ => {
                    // Other ESC sequence, skip next char
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
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
        assert!(args.contains(&"--log-junit".to_string()));
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

    #[test]
    fn test_strip_ansi_codes() {
        assert_eq!(strip_ansi_codes("\x1b[90mTests:\x1b[39m"), "Tests:");
        assert_eq!(strip_ansi_codes("\x1b[32;1m1 passed\x1b[39;22m"), "1 passed");
        assert_eq!(strip_ansi_codes("no escapes here"), "no escapes here");
        assert_eq!(strip_ansi_codes("\x1b]3;title\x07rest"), "rest");
    }

    #[test]
    fn test_parse_junit_results_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="Tests\Feature\AuthTest">
    <testcase name="it can login" class="Tests\Feature\AuthTest" time="0.1"/>
    <testcase name="it rejects bad password" class="Tests\Feature\AuthTest" time="0.05">
      <failure>Expected 401, got 200</failure>
    </testcase>
  </testsuite>
</testsuites>"#;

        // Write to temp file and parse
        let tmp = tempfile::tempdir().unwrap();
        let pesticide_dir = tmp.path().join(".pesticide");
        std::fs::create_dir_all(&pesticide_dir).unwrap();
        std::fs::write(pesticide_dir.join("results.xml"), xml).unwrap();

        let results = parse_junit_results(tmp.path());
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "it can login");
        assert_eq!(results[0].status, TestStatus::Passed);
        assert_eq!(results[0].class, Some("Tests\\Feature\\AuthTest".to_string()));
        assert_eq!(results[1].name, "it rejects bad password");
        assert_eq!(results[1].status, TestStatus::Failed);
    }
}
