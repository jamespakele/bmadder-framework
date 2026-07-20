use bmadder_core::story::Story;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Result of verifying a story against its frozen spec.
#[derive(Debug)]
pub struct VerificationResult {
    pub passed: bool,
    pub checks: Vec<VerificationCheck>,
}

#[derive(Debug)]
pub struct VerificationCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

impl VerificationResult {
    pub fn failures(&self) -> Vec<&VerificationCheck> {
        self.checks.iter().filter(|c| !c.passed).collect()
    }
}

/// Extract the "spec" portion of a story: Requirements + Acceptance Criteria + Tasks.
/// This is what gets frozen at PO approval time.
fn extract_spec_content(story: &Story) -> String {
    let mut spec = String::new();
    spec.push_str(&format!(
        "# Frozen Spec: {}\n\n",
        story.frontmatter.story_id
    ));
    spec.push_str(&format!("## Title\n{}\n\n", story.frontmatter.title));
    spec.push_str(&format!(
        "## Frozen At\n{}\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));

    // Extract sections by header
    let body = &story.body;
    let sections = parse_sections(body);

    for header in &["Requirements", "Acceptance Criteria", "Tasks", "Context"] {
        if let Some(content) = sections.get(*header) {
            spec.push_str(&format!("## {}\n{}\n\n", header, content));
        }
    }

    spec
}

/// Parse markdown body into a map of section header → content.
fn parse_sections(body: &str) -> std::collections::HashMap<String, String> {
    let mut sections = std::collections::HashMap::new();
    let mut current_header: Option<String> = None;
    let mut current_content = String::new();

    for line in body.lines() {
        // handle both ## and ### headers
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            if let Some(ref header) = current_header {
                sections.insert(header.clone(), current_content.trim().to_string());
            }
            current_header = Some(trimmed[3..].trim().to_string());
            current_content = String::new();
        } else if let Some(ref _header) = current_header {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if let Some(ref header) = current_header {
        sections.insert(header.clone(), current_content.trim().to_string());
    }

    sections
}

/// Freeze a story's spec to disk after PO approval.
/// Creates `_bmad/frozen/{story_id}.md` with the requirements + AC + tasks.
pub fn freeze_spec(
    frozen_dir: &Path,
    story: &Story,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(frozen_dir)?;

    let spec_content = extract_spec_content(story);
    let mut hasher = Sha256::new();
    hasher.update(spec_content.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let frozen_path = frozen_dir.join(format!("{}.md", story.frontmatter.story_id));

    let file_content = format!(
        "---\nstory_id: \"{}\"\nfrozen_hash: \"{}\"\nfrozen_at: \"{}\"\n---\n\n{}",
        story.frontmatter.story_id,
        hash,
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        spec_content
    );

    fs::write(&frozen_path, file_content)?;
    Ok(frozen_path)
}

/// Read a frozen spec from disk.
pub fn read_frozen_spec(frozen_dir: &Path, story_id: &str) -> Option<String> {
    let path = frozen_dir.join(format!("{}.md", story_id));
    fs::read_to_string(path).ok()
}

/// Verify a story after DEV agent returns, against its frozen spec.
/// Checks that the story has progressed meaningfully and that the
/// implementation sections are populated.
pub fn verify_after_dev(story: &Story, frozen_spec: Option<&str>) -> VerificationResult {
    let mut checks = Vec::new();
    let body = &story.body;
    let sections = parse_sections(body);

    // Check 1: Status must have moved past IN_DEV
    checks.push(VerificationCheck {
        name: "status_advanced".to_string(),
        passed: story.frontmatter.status != bmadder_core::story::StoryStatus::InDev,
        detail: format!("Status is {}", story.frontmatter.status.label()),
    });

    // Check 2: Implementation Notes must exist and be non-empty
    let impl_notes = sections
        .get("Implementation Notes")
        .or_else(|| sections.get("Dev Agent Record"))
        .cloned()
        .unwrap_or_default();
    let impl_ok = !impl_notes.trim().is_empty();
    checks.push(VerificationCheck {
        name: "implementation_notes".to_string(),
        passed: impl_ok,
        detail: if impl_ok {
            "Implementation Notes populated".to_string()
        } else {
            "Implementation Notes section missing or empty".to_string()
        },
    });

    // Check 3: File List must exist and have entries.
    // Falls back to scanning Implementation Notes for file-path-like lines
    // (e.g. "- src/lib.rs", "- assets/css/theme.css") when the story
    // template doesn't use a dedicated ## File List section.
    let file_list = sections.get("File List").cloned().unwrap_or_default();
    let file_list_count = file_list.lines().filter(|l| !l.trim().is_empty()).count();
    let (files_ok, files_detail) = if file_list_count > 0 {
        (true, format!("{} file(s) listed", file_list_count))
    } else if !impl_notes.trim().is_empty() {
        // Fallback: look for file-path-like bullet entries in Implementation Notes
        let file_like: Vec<&str> = impl_notes
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("- ") && looks_like_file_path(t[2..].trim())
            })
            .collect();
        if !file_like.is_empty() {
            (
                true,
                format!("{} file(s) found in Implementation Notes", file_like.len()),
            )
        } else {
            (
                false,
                "No file list found (neither ## File List nor file paths in Implementation Notes)"
                    .to_string(),
            )
        }
    } else {
        (
            false,
            "File List section missing and Implementation Notes empty".to_string(),
        )
    };
    checks.push(VerificationCheck {
        name: "file_list".to_string(),
        passed: files_ok,
        detail: files_detail,
    });

    // Check 4: All acceptance criteria checkboxes are checked.
    // If the AC section uses Given/When/Then prose (no checkboxes at all),
    // we skip this check — only fail when checkboxes EXIST and some are unchecked.
    let ac_section = sections
        .get("Acceptance Criteria")
        .cloned()
        .unwrap_or_default();
    let tasks_section = sections
        .get("Tasks")
        .or_else(|| sections.get("Tasks/Subtasks"))
        .cloned()
        .unwrap_or_default();
    let checkable = ac_section
        .lines()
        .chain(tasks_section.lines())
        .filter(|l| l.trim_start().starts_with("- [ ]") || l.trim_start().starts_with("- [x]"));
    let total: usize = checkable.clone().count();
    let done: usize = checkable
        .filter(|l| l.trim_start().starts_with("- [x]"))
        .count();
    // Pass if: all checkboxes checked, OR no checkboxes at all (prose-style AC)
    let ac_ok = total == 0 || done == total;
    let ac_detail = if total == 0 {
        "No checkboxes found — prose-style AC, skipping checkbox check".to_string()
    } else {
        format!("{}/{} acceptance criteria checked", done, total)
    };
    checks.push(VerificationCheck {
        name: "acceptance_criteria".to_string(),
        passed: ac_ok,
        detail: ac_detail,
    });

    // Check 5: Frozen spec must still exist (if provided)
    if let Some(spec) = frozen_spec {
        let frozen_sections = parse_sections(spec);
        let frozen_ac = frozen_sections
            .get("Acceptance Criteria")
            .cloned()
            .unwrap_or_default();
        let current_ac = ac_section;
        let drift = frozen_ac.trim() != current_ac.trim() && !frozen_ac.trim().is_empty();
        checks.push(VerificationCheck {
            name: "spec_drift".to_string(),
            passed: !drift,
            detail: if drift {
                "Acceptance Criteria have drifted from frozen spec".to_string()
            } else {
                "Acceptance Criteria match frozen spec".to_string()
            },
        });
    }

    let passed = checks.iter().all(|c| c.passed);
    VerificationResult { passed, checks }
}

/// Heuristic: does a string look like a file path?
/// Matches paths with extensions or with path separators.
fn looks_like_file_path(s: &str) -> bool {
    // Strip backticks and leading/trailing whitespace
    let s = s.trim().trim_matches('`');
    if s.is_empty() {
        return false;
    }
    // Contains a path separator and either an extension or looks path-like
    s.contains('/') && (s.contains('.') || s.contains('/')) || (s.contains('.') && !s.contains(' '))
}

/// Verify a story after QA agent returns.
pub fn verify_after_qa(story: &Story) -> VerificationResult {
    let mut checks = Vec::new();
    let body = &story.body;
    let sections = parse_sections(body);

    // Check 1: Status must be COMPLETED
    let status_ok = story.frontmatter.status == bmadder_core::story::StoryStatus::Completed;
    checks.push(VerificationCheck {
        name: "status_completed".to_string(),
        passed: status_ok,
        detail: format!("Status is {}", story.frontmatter.status.label()),
    });

    // Check 2: qa_status must be PASS
    let qa_status_ok = story
        .frontmatter
        .qa_status
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("PASS"))
        .unwrap_or(false);
    checks.push(VerificationCheck {
        name: "qa_status_pass".to_string(),
        passed: qa_status_ok,
        detail: format!("qa_status = {:?}", story.frontmatter.qa_status),
    });

    // Check 3: QA Notes must exist and be non-empty
    let qa_notes = sections
        .get("QA Notes")
        .or_else(|| sections.get("Quality Assurance Notes"))
        .cloned()
        .unwrap_or_default();
    let qa_notes_ok = !qa_notes.trim().is_empty();
    checks.push(VerificationCheck {
        name: "qa_notes".to_string(),
        passed: qa_notes_ok,
        detail: if qa_notes_ok {
            "QA Notes populated".to_string()
        } else {
            "QA Notes section missing or empty".to_string()
        },
    });

    let passed = checks.iter().all(|c| c.passed);
    VerificationResult { passed, checks }
}

/// Append a deferred work entry to the ledger.
pub fn log_deferred(
    deferred_path: &Path,
    story_id: &str,
    summary: &str,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = deferred_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Read existing to get next sequence number
    let existing = fs::read_to_string(deferred_path).unwrap_or_default();
    let next_id = existing
        .lines()
        .filter(|l| l.starts_with("### DW-"))
        .count()
        + 1;

    let entry = format!(
        "### DW-{}: {}\n\norigin: {} · {}\nlocation: n/a\nreason: {}\nstatus: open\n\n---\n\n",
        next_id,
        summary,
        story_id,
        chrono::Utc::now().format("%Y-%m-%d"),
        reason,
    );

    // Append (create if missing)
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(deferred_path)?;
    use std::io::Write;
    file.write_all(entry.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmadder_core::story::{StoryFrontmatter, StoryStatus};
    use std::path::PathBuf;

    fn make_story(body: &str, status: StoryStatus) -> Story {
        Story {
            path: PathBuf::from("test.md"),
            frontmatter: StoryFrontmatter {
                story_id: "STORY-0001".into(),
                title: "Test".into(),
                status,
                epic_id: None,
                priority: None,
                agent_hint: None,
                assigned_dev: None,
                po_alignment: None,
                qa_status: None,
                created_at: None,
                updated_at: None,
                links: vec![],
            },
            body: body.to_string(),
        }
    }

    #[test]
    fn verify_dev_passes_with_populated_sections() {
        let body = r#"
## Requirements
Do the thing.

## Acceptance Criteria
- [x] AC1 works
- [x] AC2 works

## Implementation Notes
Implemented the thing in src/lib.rs

## File List
- src/lib.rs
- tests/thing.rs

## Tasks
- [x] Task 1 done
"#;
        let story = make_story(body, StoryStatus::PendingQA);
        let result = verify_after_dev(&story, None);
        assert!(result.passed, "Expected pass: {:?}", result.failures());
    }

    #[test]
    fn verify_dev_fails_with_empty_implementation_notes() {
        let body = r#"
## Acceptance Criteria
- [x] AC1 works

## Implementation Notes

## File List
- src/lib.rs
"#;
        let story = make_story(body, StoryStatus::InDev);
        let result = verify_after_dev(&story, None);
        assert!(!result.passed);
        assert!(result
            .failures()
            .iter()
            .any(|c| c.name == "implementation_notes"));
    }

    #[test]
    fn verify_dev_fails_with_unchecked_acceptance() {
        let body = r#"
## Acceptance Criteria
- [x] AC1 works
- [ ] AC2 not done

## Implementation Notes
Partial implementation

## File List
- src/lib.rs
"#;
        let story = make_story(body, StoryStatus::PendingQA);
        let result = verify_after_dev(&story, None);
        assert!(!result.passed);
        assert!(result
            .failures()
            .iter()
            .any(|c| c.name == "acceptance_criteria"));
    }

    #[test]
    fn verify_dev_passes_with_prose_ac_no_checkboxes() {
        // Stories with Given/When/Then prose ACs and no checkboxes
        // should pass the acceptance_criteria check.
        let body = r#"
## Acceptance Criteria

**Given** the theme file exists
**When** the story is implemented
**Then** both themes are defined

## Implementation Notes
Implemented theme bundles.

- assets/css/theme.css
- src/templates/layout.rs
"#;
        let story = make_story(body, StoryStatus::PendingQA);
        let result = verify_after_dev(&story, None);
        assert!(result.passed, "Expected pass: {:?}", result.failures());
    }

    #[test]
    fn verify_dev_passes_with_file_list_in_impl_notes() {
        // Stories without a ## File List section but with file paths
        // listed in ## Implementation Notes should pass the file_list check.
        let body = r#"
## Acceptance Criteria
- [x] AC1 works

## Implementation Notes
Implemented the thing.

- src/lib.rs
- tests/thing.rs
"#;
        let story = make_story(body, StoryStatus::PendingQA);
        let result = verify_after_dev(&story, None);
        assert!(result.passed, "Expected pass: {:?}", result.failures());
    }

    #[test]
    fn verify_dev_fails_with_no_files_anywhere() {
        // No ## File List and no file paths in Implementation Notes
        let body = r#"
## Acceptance Criteria
- [x] AC1 works

## Implementation Notes
Implemented the thing. No files listed.
"#;
        let story = make_story(body, StoryStatus::PendingQA);
        let result = verify_after_dev(&story, None);
        assert!(!result.passed);
        assert!(result.failures().iter().any(|c| c.name == "file_list"));
    }

    #[test]
    fn verify_qa_passes_with_completed_status() {
        let body = r#"
## QA Notes
All tests pass. No regressions found.
"#;
        let mut story = make_story(body, StoryStatus::Completed);
        story.frontmatter.qa_status = Some("PASS".into());
        let result = verify_after_qa(&story);
        assert!(result.passed, "Expected pass: {:?}", result.failures());
    }

    #[test]
    fn verify_qa_fails_without_pass_status() {
        let body = r#"
## QA Notes
Tests failed.
"#;
        let mut story = make_story(body, StoryStatus::Refix);
        story.frontmatter.qa_status = Some("FAIL".into());
        let result = verify_after_qa(&story);
        assert!(!result.passed);
        assert!(result
            .failures()
            .iter()
            .any(|c| c.name == "status_completed"));
    }

    #[test]
    fn freeze_and_read_spec() {
        let dir = tempfile::tempdir().unwrap();
        let frozen_dir = dir.path().join("frozen");
        let body = r#"
## Requirements
Build auth.

## Acceptance Criteria
- [ ] Login works
- [ ] Logout works

## Context
Auth module for the app.
"#;
        let story = make_story(body, StoryStatus::ReadyForDev);
        let path = freeze_spec(&frozen_dir, &story).unwrap();
        assert!(path.exists());

        let content = read_frozen_spec(&frozen_dir, "STORY-0001").unwrap();
        assert!(content.contains("Build auth."));
        assert!(content.contains("Login works"));
        assert!(content.contains("frozen_hash"));
    }
}
