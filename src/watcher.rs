use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, Debouncer};

/// Events produced by the file watcher for consumption by the main event loop.
pub enum WatchEvent {
    /// A test file was modified (the file still exists on disk).
    TestFileChanged(PathBuf),
    /// A source file under app/ was modified.
    SourceFileChanged(PathBuf),
    /// A test file was created or deleted (file no longer exists at that path).
    TestFileCreatedOrDeleted,
}

/// Directories/files that should be ignored by the watcher.
const IGNORED_SEGMENTS: &[&str] = &[".git/", "vendor/", "node_modules/", ".pesticide/"];

/// Starts a file-system watcher rooted at `project_root`.
///
/// Debounces events with a 500ms delay. For each debounced event, classifies
/// the changed path and sends a [`WatchEvent`] on `tx`.
///
/// The caller **must** keep the returned `Debouncer` alive for as long as
/// watching should remain active. Dropping it stops the watcher.
pub fn start_watcher(
    project_root: &Path,
    tx: mpsc::Sender<WatchEvent>,
) -> Result<Debouncer<notify::RecommendedWatcher>> {
    let root = project_root.to_path_buf();

    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |res: notify_debouncer_mini::DebounceEventResult| {
            let events = match res {
                Ok(events) => events,
                Err(_) => return,
            };

            for event in events {
                let path: PathBuf = event.path;

            // Convert the path to a string for segment matching.
            let path_str = path.to_string_lossy();

            // Skip ignored directories.
            if IGNORED_SEGMENTS.iter().any(|seg| path_str.contains(seg)) {
                continue;
            }

            // Determine the relative path from the project root for classification.
            let rel = path
                .strip_prefix(&root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path_str.to_string());

            // Classify the event.
            let is_test_path = rel.contains("tests/")
                || rel.starts_with("tests")
                || (rel.contains("app-modules/") && rel.contains("tests/"));

            if is_test_path {
                let watch_event = if path.exists() {
                    WatchEvent::TestFileChanged(path)
                } else {
                    WatchEvent::TestFileCreatedOrDeleted
                };
                let _ = tx.send(watch_event);
            } else if rel.contains("app/") || rel.starts_with("app") {
                let _ = tx.send(WatchEvent::SourceFileChanged(path));
            }
        }
    })?;

    debouncer
        .watcher()
        .watch(project_root, RecursiveMode::Recursive)?;

    Ok(debouncer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn test_ignored_segments() {
        // Verify the constant is defined correctly.
        assert!(IGNORED_SEGMENTS.contains(&".git/"));
        assert!(IGNORED_SEGMENTS.contains(&"vendor/"));
        assert!(IGNORED_SEGMENTS.contains(&"node_modules/"));
        assert!(IGNORED_SEGMENTS.contains(&".pesticide/"));
    }

    #[test]
    fn test_watcher_detects_test_file_change() {
        let tmp = tempfile::tempdir().unwrap();
        // Canonicalize to resolve macOS /var -> /private/var symlink
        let root = tmp.path().canonicalize().unwrap();
        let tests_dir = root.join("tests/Feature");
        fs::create_dir_all(&tests_dir).unwrap();

        let (tx, rx) = mpsc::channel();
        let _debouncer = start_watcher(&root, tx).unwrap();

        // Write a file into tests/
        let test_file = tests_dir.join("ExampleTest.php");
        fs::write(&test_file, "<?php // test").unwrap();

        // Wait for the debounced event (500ms debounce + some slack)
        let event = rx.recv_timeout(Duration::from_secs(5));
        assert!(event.is_ok(), "Expected a watch event but got timeout");

        match event.unwrap() {
            WatchEvent::TestFileChanged(p) => {
                assert_eq!(p, test_file);
            }
            _ => panic!("Expected TestFileChanged event"),
        }
    }

    #[test]
    fn test_watcher_detects_source_file_change() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let app_dir = root.join("app/Models");
        fs::create_dir_all(&app_dir).unwrap();

        let (tx, rx) = mpsc::channel();
        let _debouncer = start_watcher(&root, tx).unwrap();

        // Write a file into app/
        let src_file = app_dir.join("User.php");
        fs::write(&src_file, "<?php // model").unwrap();

        let event = rx.recv_timeout(Duration::from_secs(5));
        assert!(event.is_ok(), "Expected a watch event but got timeout");

        match event.unwrap() {
            WatchEvent::SourceFileChanged(p) => {
                assert_eq!(p, src_file);
            }
            _ => panic!("Expected SourceFileChanged event"),
        }
    }

    #[test]
    fn test_watcher_ignores_vendor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let vendor_dir = root.join("vendor/some-package");
        fs::create_dir_all(&vendor_dir).unwrap();

        let (tx, rx) = mpsc::channel();
        let _debouncer = start_watcher(&root, tx).unwrap();

        // Write a file into vendor/
        let vendor_file = vendor_dir.join("something.php");
        fs::write(&vendor_file, "<?php // vendor").unwrap();

        // Should NOT receive an event within a reasonable timeout
        let event = rx.recv_timeout(Duration::from_secs(2));
        assert!(event.is_err(), "Should not receive events for vendor/ files");
    }

    #[test]
    fn test_watcher_test_file_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let tests_dir = root.join("tests/Feature");
        fs::create_dir_all(&tests_dir).unwrap();

        let test_file = tests_dir.join("DeleteMe.php");
        fs::write(&test_file, "<?php // will be deleted").unwrap();

        let (tx, rx) = mpsc::channel();
        let _debouncer = start_watcher(&root, tx).unwrap();

        // Give watcher time to register initial state, then delete the file
        std::thread::sleep(Duration::from_millis(200));
        fs::remove_file(&test_file).unwrap();

        let event = rx.recv_timeout(Duration::from_secs(5));
        assert!(event.is_ok(), "Expected a watch event but got timeout");

        match event.unwrap() {
            WatchEvent::TestFileCreatedOrDeleted => {
                // correct
            }
            _ => panic!("Expected TestFileCreatedOrDeleted event"),
        }
    }
}
