use crate::logging;
use crate::story_io;
use bmadder_core::config::Config;
use bmadder_core::story::StoryStatus;
use std::fs;

/// Triage the deferred-work ledger into actionable categories.
///
/// Reads `_bmad/deferred-work.md` and partitions entries into:
/// - **buildable**: can be turned into stories for the next dev cycle
/// - **already_resolved**: the issue appears fixed in the codebase
/// - **blocked**: needs human input or external dependency
/// - **skip**: outdated or no longer relevant
///
/// For buildable entries, optionally creates draft story files.
pub fn run_sweep(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Sweep (Deferred Work Triage)");

    let ledger_path = &config.paths.deferred_work_file;
    if !ledger_path.exists() {
        logging::info("No deferred-work ledger found. Nothing to sweep.");
        logging::log_progress(config, "SWEEP: no ledger")?;
        return Ok(());
    }

    let content = fs::read_to_string(ledger_path)?;
    let entries = parse_deferred_work(&content);

    if entries.is_empty() {
        logging::info("Deferred-work ledger is empty. Nothing to sweep.");
        logging::log_progress(config, "SWEEP: empty ledger")?;
        return Ok(());
    }

    logging::info(&format!("Found {} deferred-work entries.", entries.len()));

    let mut buildable = Vec::new();
    let mut blocked = Vec::new();
    let mut skip = Vec::new();
    let mut already_resolved = Vec::new();

    for entry in &entries {
        if entry.status == "done" {
            already_resolved.push(entry);
        } else if entry.status == "open" {
            // Simple heuristic: entries with "blocked" in reason → blocked
            // Entries with "deprecated" or "obsolete" → skip
            // Everything else → buildable
            let reason_lower = entry.reason.to_lowercase();
            if reason_lower.contains("blocked") || reason_lower.contains("dependency") {
                blocked.push(entry);
            } else if reason_lower.contains("deprecated")
                || reason_lower.contains("obsolete")
                || reason_lower.contains("no longer")
            {
                skip.push(entry);
            } else {
                buildable.push(entry);
            }
        }
    }

    // Report
    logging::info(&format!("  Buildable:       {}", buildable.len()));
    logging::info(&format!("  Already resolved: {}", already_resolved.len()));
    logging::info(&format!("  Blocked:         {}", blocked.len()));
    logging::info(&format!("  Skip:            {}", skip.len()));

    if !buildable.is_empty() {
        logging::ok(&format!(
            "{} buildable entries ready for stories.",
            buildable.len()
        ));
        for entry in &buildable {
            logging::info(&format!("  {} : {}", entry.id, entry.title));
        }

        // Check existing stories to avoid duplicates
        let existing_stories = story_io::list_stories(&config.paths.stories_dir)?;
        let existing_ids: Vec<String> = existing_stories
            .iter()
            .filter_map(|p| {
                story_io::parse_story_file(p)
                    .ok()
                    .map(|s| s.frontmatter.story_id)
            })
            .collect();

        // Find next story number
        let next_num = existing_stories.len() + 1;
        let mut story_num = next_num;

        for entry in &buildable {
            // Skip if a story already references this DW entry
            let already_has_story = existing_ids.iter().any(|id| id.contains(&entry.id));
            if already_has_story {
                logging::info(&format!("  {} already has a story. Skipping.", entry.id));
                continue;
            }

            // Create a draft story from this deferred-work entry
            let story_id = format!("STORY-{:04}", story_num);
            let slug = entry
                .title
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .trim_matches('-')
                .to_string();
            let filename = format!("story-{:04}-{}.md", story_num, slug);
            let story_path = config.paths.stories_dir.join(&filename);

            let story_content = format!(
                r#"---
story_id: "{story_id}"
title: "{title}"
status: DRAFT
po_alignment: "PENDING"
agent_hint: "generalist"
---

## Context

Deferred work entry: {dw_id}

## Requirements

{reason}

## Acceptance Criteria

- [ ] {title} is implemented
- [ ] Tests pass
- [ ] No regressions

## Implementation Notes

(TBD — DEV agent will populate)

## PO Alignment

PENDING — auto-generated from deferred-work sweep

## QA Notes

(TBD — QA agent will populate)

## Tasks

- [ ] Implement {title}
"#,
                story_id = story_id,
                title = entry.title,
                dw_id = entry.id,
                reason = entry.reason,
            );

            fs::create_dir_all(&config.paths.stories_dir)?;
            fs::write(&story_path, story_content)?;

            logging::ok(&format!(
                "Created story: {} ({})",
                story_id,
                story_path.display()
            ));
            logging::log_activity(
                config,
                "SWEEP",
                &story_id,
                "STORY_CREATED",
                &format!("From {} : {}", entry.id, entry.title),
            )?;

            story_num += 1;
        }
    }

    if !blocked.is_empty() {
        logging::warn(&format!(
            "{} blocked entries need human input:",
            blocked.len()
        ));
        for entry in &blocked {
            logging::warn(&format!(
                "  {} : {} — {}",
                entry.id, entry.title, entry.reason
            ));
        }
    }

    if !skip.is_empty() {
        logging::info(&format!("{} entries marked for skip:", skip.len()));
        for entry in &skip {
            logging::info(&format!("  {} : {}", entry.id, entry.title));
        }
    }

    logging::log_progress(
        config,
        &format!(
            "SWEEP: {} buildable, {} blocked, {} skip, {} resolved",
            buildable.len(),
            blocked.len(),
            skip.len(),
            already_resolved.len()
        ),
    )?;

    Ok(())
}

struct DeferredWorkEntry {
    id: String,
    title: String,
    reason: String,
    status: String,
}

fn parse_deferred_work(content: &str) -> Vec<DeferredWorkEntry> {
    let mut entries = Vec::new();
    let mut current: Option<DeferredWorkEntry> = None;

    for line in content.lines() {
        if line.starts_with("### DW-") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            let title = line.trim_start_matches("### DW-").trim().to_string();
            let id = line
                .split(':')
                .next()
                .unwrap_or("")
                .trim_start_matches("### ")
                .to_string();
            current = Some(DeferredWorkEntry {
                id,
                title,
                reason: String::new(),
                status: "open".to_string(),
            });
        } else if let Some(ref mut entry) = current {
            let trimmed = line.trim();
            if trimmed.starts_with("reason:") {
                entry.reason = trimmed.trim_start_matches("reason:").trim().to_string();
            } else if trimmed.starts_with("status:") {
                entry.status = trimmed.trim_start_matches("status:").trim().to_string();
            }
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    entries
}
