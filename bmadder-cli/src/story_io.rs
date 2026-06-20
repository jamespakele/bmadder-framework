use bmadder_core::story::{Story, StoryFrontmatter, StoryStatus};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Walk up from `start_dir` looking for `bmadder.toml`.
pub fn find_config(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        let candidate = current.join("bmadder.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Parse a story file: YAML frontmatter between `---` fences, then markdown body.
pub fn parse_story_file(path: &Path) -> Result<Story, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;

    // Detect frontmatter fences
    let mut lines = content.lines();
    let first = lines.next().unwrap_or("");
    if first.trim() != "---" {
        return Err(format!("{}: missing opening frontmatter fence", path.display()).into());
    }

    let mut yaml_lines = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        yaml_lines.push(line);
    }

    // body_start is relative to after the first line; compute absolute offset
    let header_len = first.len() + 1; // first "---" + newline
    let yaml_block_len: usize = yaml_lines.iter().map(|l| l.len() + 1).sum();
    let closing_fence_len = 3; // "---"
    let body = content[header_len + yaml_block_len + closing_fence_len..].to_string();

    let yaml_str = yaml_lines.join("\n");
    let mut frontmatter: StoryFrontmatter = serde_yaml::from_str(&yaml_str)?;

    // If story_id is missing (LLM wrote `slug` or omitted it), derive from filename.
    // story-0009-slash-command-palette.md → STORY-0009
    if frontmatter.story_id.is_empty() {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let digits: String = stem
                .split('-')
                .find(|part| part.chars().all(|c| c.is_ascii_digit()) && !part.is_empty())
                .unwrap_or("")
                .to_string();
            if !digits.is_empty() {
                frontmatter.story_id = format!("STORY-{}", digits);
            } else {
                frontmatter.story_id = stem.to_uppercase().replace('-', "_");
            }
        }
    }

    Ok(Story {
        path: path.to_path_buf(),
        frontmatter,
        body,
    })
}

/// Write a story file back to disk.
pub fn write_story_file(story: &Story) -> Result<(), Box<dyn std::error::Error>> {
    let yaml = serde_yaml::to_string(&story.frontmatter)?;
    let content = format!("---\n{}---\n{}", yaml, story.body);
    fs::write(&story.path, content)?;
    Ok(())
}

/// Update a story's status in its frontmatter.
pub fn update_story_status(
    path: &Path,
    new_status: StoryStatus,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut story = parse_story_file(path)?;
    story.frontmatter.status = new_status;
    write_story_file(&story)
}

/// Update a single frontmatter string field by name.
pub fn update_story_field(
    path: &Path,
    field: &str,
    value: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut story = parse_story_file(path)?;
    match field {
        "status" => {
            story.frontmatter.status = serde_yaml::from_str(value)?;
        }
        "po_alignment" => story.frontmatter.po_alignment = Some(value.to_string()),
        "qa_status" => story.frontmatter.qa_status = Some(value.to_string()),
        "assigned_dev" => story.frontmatter.assigned_dev = Some(value.to_string()),
        "title" => story.frontmatter.title = value.to_string(),
        _ => return Err(format!("unknown field: {}", field).into()),
    }
    write_story_file(&story)
}

/// List all story files in the stories directory, sorted by filename.
pub fn list_stories(stories_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    if !stories_dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(stories_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("story-") && n.ends_with(".md"))
                    .unwrap_or(false)
        })
        .collect();
    paths.sort();
    Ok(paths)
}

/// Get all stories with a given status.
pub fn get_stories_by_status(
    stories_dir: &Path,
    status: StoryStatus,
) -> Result<Vec<Story>, Box<dyn std::error::Error>> {
    let paths = list_stories(stories_dir)?;
    let mut stories = Vec::new();
    for path in paths {
        let story = parse_story_file(&path)?;
        if story.frontmatter.status == status {
            stories.push(story);
        }
    }
    Ok(stories)
}

/// Count stories at a given status.
pub fn count_by_status(stories_dir: &Path, status: StoryStatus) -> usize {
    get_stories_by_status(stories_dir, status)
        .map(|s| s.len())
        .unwrap_or(0)
}

/// Filter stories to only those matching a target story_id.
pub fn filter_stories_by_id(stories: Vec<Story>, target_id: &str) -> Vec<Story> {
    stories
        .into_iter()
        .filter(|s| s.frontmatter.story_id == target_id)
        .collect()
}

/// Filter stories to only those at or after a given story_id (by filename sort order).
pub fn filter_stories_from_id(stories: Vec<Story>, start_from: &str) -> Vec<Story> {
    let mut reached = false;
    stories
        .into_iter()
        .filter(|s| {
            if s.frontmatter.story_id == start_from {
                reached = true;
            }
            reached
        })
        .collect()
}

/// Validate all story files. Returns a list of error messages.
pub fn validate_stories(stories_dir: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let paths = list_stories(stories_dir)?;
    let mut errors = Vec::new();
    for path in paths {
        match parse_story_file(&path) {
            Ok(story) => {
                let fm = &story.frontmatter;
                if fm.story_id.is_empty() {
                    errors.push(format!(
                        "{}: missing story_id",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                if fm.title.is_empty() {
                    errors.push(format!(
                        "{}: missing title",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                // Status validity is enforced by the enum deserialization
            }
            Err(e) => {
                errors.push(format!(
                    "{}: parse error: {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    e
                ));
            }
        }
    }
    Ok(errors)
}

/// Detect a newly created story file by diffing two directory listings.
pub fn detect_new_story_file(before: &[PathBuf], after: &[PathBuf]) -> Option<PathBuf> {
    let before_set: HashSet<&PathBuf> = before.iter().collect();
    after.iter().find(|p| !before_set.contains(p)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_story(path: &Path, story_id: &str, status: StoryStatus) {
        let content = format!(
            r#"---
story_id: "{}"
title: "Test Story"
status: {}
---

## Context
Test body content.
"#,
            story_id,
            status.label()
        );
        fs::write(path, content).unwrap();
    }

    #[test]
    fn parse_and_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("story-0001-test.md");
        write_story(&path, "STORY-0001", StoryStatus::Draft);

        let story = parse_story_file(&path).unwrap();
        assert_eq!(story.frontmatter.story_id, "STORY-0001");
        assert_eq!(story.frontmatter.status, StoryStatus::Draft);
        assert!(story.body.contains("Test body content"));

        // Write back and re-read
        write_story_file(&story).unwrap();
        let story2 = parse_story_file(&path).unwrap();
        assert_eq!(story2.frontmatter.story_id, "STORY-0001");
        assert_eq!(story2.frontmatter.status, StoryStatus::Draft);
        assert!(story2.body.contains("Test body content"));
    }

    #[test]
    fn update_status() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("story-0001-test.md");
        write_story(&path, "STORY-0001", StoryStatus::Draft);

        update_story_status(&path, StoryStatus::ReadyForDev).unwrap();
        let story = parse_story_file(&path).unwrap();
        assert_eq!(story.frontmatter.status, StoryStatus::ReadyForDev);
    }

    #[test]
    fn list_and_filter_by_status() {
        let dir = tempfile::tempdir().unwrap();
        let stories_dir = dir.path().join("stories");
        fs::create_dir_all(&stories_dir).unwrap();

        write_story(
            &stories_dir.join("story-0001-auth.md"),
            "STORY-0001",
            StoryStatus::Draft,
        );
        write_story(
            &stories_dir.join("story-0002-db.md"),
            "STORY-0002",
            StoryStatus::ReadyForDev,
        );
        write_story(
            &stories_dir.join("story-0003-api.md"),
            "STORY-0003",
            StoryStatus::Draft,
        );

        let drafts = get_stories_by_status(&stories_dir, StoryStatus::Draft).unwrap();
        assert_eq!(drafts.len(), 2);

        let ready = get_stories_by_status(&stories_dir, StoryStatus::ReadyForDev).unwrap();
        assert_eq!(ready.len(), 1);

        assert_eq!(count_by_status(&stories_dir, StoryStatus::Draft), 2);
        assert_eq!(count_by_status(&stories_dir, StoryStatus::Completed), 0);
    }

    #[test]
    fn filter_by_story_id() {
        let dir = tempfile::tempdir().unwrap();
        let stories_dir = dir.path().join("stories");
        fs::create_dir_all(&stories_dir).unwrap();

        write_story(
            &stories_dir.join("story-0001-auth.md"),
            "STORY-0001",
            StoryStatus::Draft,
        );
        write_story(
            &stories_dir.join("story-0002-db.md"),
            "STORY-0002",
            StoryStatus::Draft,
        );

        let all = get_stories_by_status(&stories_dir, StoryStatus::Draft).unwrap();
        let filtered = filter_stories_by_id(all, "STORY-0002");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].frontmatter.story_id, "STORY-0002");
    }

    #[test]
    fn filter_from_id() {
        let dir = tempfile::tempdir().unwrap();
        let stories_dir = dir.path().join("stories");
        fs::create_dir_all(&stories_dir).unwrap();

        for i in 1..=5 {
            write_story(
                &stories_dir.join(format!("story-000{}-test.md", i)),
                &format!("STORY-000{}", i),
                StoryStatus::Draft,
            );
        }

        let all = get_stories_by_status(&stories_dir, StoryStatus::Draft).unwrap();
        let from_3 = filter_stories_from_id(all, "STORY-0003");
        assert_eq!(from_3.len(), 3);
        assert_eq!(from_3[0].frontmatter.story_id, "STORY-0003");
        assert_eq!(from_3[2].frontmatter.story_id, "STORY-0005");
    }

    #[test]
    fn validate_stories_detects_issues() {
        let dir = tempfile::tempdir().unwrap();
        let stories_dir = dir.path().join("stories");
        fs::create_dir_all(&stories_dir).unwrap();

        write_story(
            &stories_dir.join("story-0001-ok.md"),
            "STORY-0001",
            StoryStatus::Draft,
        );

        // Write a file with missing story_id
        let bad = stories_dir.join("story-0002-bad.md");
        fs::write(&bad, "---\ntitle: \"Bad\"\nstatus: DRAFT\n---\n\nbody\n").unwrap();

        let errors = validate_stories(&stories_dir).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("story_id"));
    }

    #[test]
    fn detect_new_file() {
        let before = vec![
            PathBuf::from("story-0001.md"),
            PathBuf::from("story-0002.md"),
        ];
        let after = vec![
            PathBuf::from("story-0001.md"),
            PathBuf::from("story-0002.md"),
            PathBuf::from("story-0003.md"),
        ];
        let new = detect_new_story_file(&before, &after).unwrap();
        assert_eq!(new, PathBuf::from("story-0003.md"));
    }

    #[test]
    fn detect_no_new_file() {
        let before = vec![PathBuf::from("story-0001.md")];
        let after = vec![PathBuf::from("story-0001.md")];
        assert!(detect_new_story_file(&before, &after).is_none());
    }
}
