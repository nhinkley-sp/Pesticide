use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::app::{CoverageFile, CoverageSourceLine, LineCoverageStatus};

/// Parse a Clover XML coverage report and return a Vec of CoverageFile summaries.
///
/// Finds all `<file name="...">` elements, counts `<line>` elements with
/// type "stmt", "method", or "cond", and calculates hit/miss/percentage.
pub fn parse_clover_xml(xml: &str) -> Result<Vec<CoverageFile>> {
    let doc = roxmltree::Document::parse(xml)
        .context("Failed to parse Clover XML")?;

    let mut files = Vec::new();

    for node in doc.descendants() {
        if node.is_element() && node.tag_name().name() == "file" {
            let path = match node.attribute("name") {
                Some(name) => name.to_string(),
                None => continue,
            };

            let mut lines = 0usize;
            let mut hits = 0usize;

            for child in node.children() {
                if !child.is_element() || child.tag_name().name() != "line" {
                    continue;
                }

                let line_type = child.attribute("type").unwrap_or("");
                if line_type != "stmt" && line_type != "method" && line_type != "cond" {
                    continue;
                }

                lines += 1;

                let count: usize = child
                    .attribute("count")
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0);

                if count > 0 {
                    hits += 1;
                }
            }

            let misses = lines.saturating_sub(hits);
            let percent = if lines > 0 {
                (hits as f64 / lines as f64) * 100.0
            } else {
                0.0
            };

            files.push(CoverageFile {
                path,
                lines,
                hits,
                misses,
                percent,
            });
        }
    }

    Ok(files)
}

/// Get line-level coverage for a specific file from a Clover XML report.
///
/// Returns a HashMap of line_number -> hit_count for all `<line>` elements
/// in the matching `<file>` element.
pub fn parse_file_line_coverage(xml: &str, file_path: &str) -> Result<HashMap<usize, usize>> {
    let doc = roxmltree::Document::parse(xml)
        .context("Failed to parse Clover XML")?;

    let mut line_hits = HashMap::new();

    for node in doc.descendants() {
        if node.is_element()
            && node.tag_name().name() == "file"
            && node.attribute("name") == Some(file_path)
        {
            for child in node.children() {
                if !child.is_element() || child.tag_name().name() != "line" {
                    continue;
                }

                let num: usize = match child.attribute("num") {
                    Some(n) => n.parse().unwrap_or(0),
                    None => continue,
                };

                let count: usize = child
                    .attribute("count")
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0);

                line_hits.insert(num, count);
            }
            break;
        }
    }

    Ok(line_hits)
}

/// Read a PHP source file and merge with coverage data to produce annotated source lines.
///
/// For each line in the file:
/// - If the line number is in `line_hits` with count > 0: `Covered`
/// - If the line number is in `line_hits` with count == 0: `Uncovered`
/// - If the line number is not in `line_hits`: `NotExecutable`
pub fn build_coverage_source(
    source_path: &Path,
    line_hits: &HashMap<usize, usize>,
) -> Result<Vec<CoverageSourceLine>> {
    let content = std::fs::read_to_string(source_path)
        .with_context(|| format!("Failed to read source file: {}", source_path.display()))?;

    let lines: Vec<CoverageSourceLine> = content
        .lines()
        .enumerate()
        .map(|(idx, line_content)| {
            let line_number = idx + 1;
            let status = match line_hits.get(&line_number) {
                Some(&count) if count > 0 => LineCoverageStatus::Covered,
                Some(_) => LineCoverageStatus::Uncovered,
                None => LineCoverageStatus::NotExecutable,
            };
            CoverageSourceLine {
                line_number,
                content: line_content.to_string(),
                status,
            }
        })
        .collect();

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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

    #[test]
    fn test_build_coverage_source() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.php");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            writeln!(f, "<?php").unwrap();
            writeln!(f, "function hello() {{").unwrap();
            writeln!(f, "    echo 'hi';").unwrap();
            writeln!(f, "}}").unwrap();
        }

        let mut line_hits = HashMap::new();
        line_hits.insert(2, 3); // covered
        line_hits.insert(3, 0); // uncovered
        // line 1 and 4 not in map => NotExecutable

        let result = build_coverage_source(&file_path, &line_hits).unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].line_number, 1);
        assert_eq!(result[0].status, LineCoverageStatus::NotExecutable);
        assert_eq!(result[1].line_number, 2);
        assert_eq!(result[1].status, LineCoverageStatus::Covered);
        assert_eq!(result[2].line_number, 3);
        assert_eq!(result[2].status, LineCoverageStatus::Uncovered);
        assert_eq!(result[3].line_number, 4);
        assert_eq!(result[3].status, LineCoverageStatus::NotExecutable);
    }

    #[test]
    fn test_parse_file_line_coverage_missing_file() {
        let hits = parse_file_line_coverage(SAMPLE_CLOVER, "nonexistent.php").unwrap();
        assert!(hits.is_empty());
    }
}
