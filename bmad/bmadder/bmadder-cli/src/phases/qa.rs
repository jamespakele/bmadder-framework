use crate::agent::invoke_agent;
use crate::git;
use crate::logging;
use crate::prompts;
use crate::story_io;
use bmadder_core::config::{Config, Phase};
use bmadder_core::story::StoryStatus;
use std::collections::HashMap;

pub fn run_qa(
    config: &Config,
    target_story: Option<&str>,
    no_commit: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Phase: QA (Quality Assurance)");

    // Queue: PENDING_QA stories
    let mut queue =
        story_io::get_stories_by_status(&config.paths.stories_dir, StoryStatus::PendingQA)?;
    queue.sort_by(|a, b| a.path.cmp(&b.path));

    // Filter by target_story if set
    if let Some(target) = target_story {
        let target = target.trim();
        if !target.is_empty() {
            queue = story_io::filter_stories_by_id(queue, target);
        }
    }

    if queue.is_empty() {
        logging::warn("No PENDING_QA stories found.");
        logging::log_progress(config, "QA: nothing to review")?;
        return Ok(());
    }

    logging::info(&format!("{} story/stories queued for QA.", queue.len()));

    let mut passed = 0usize;
    let mut failed = 0usize;

    for story in &queue {
        let story_id = &story.frontmatter.story_id;
        let title = &story.frontmatter.title;
        logging::story_banner(&format!("{}: {}", story_id, title));

        let model = config.resolve_model(Phase::QA, None);
        logging::info(&format!("QA agent model: {}", model));

        logging::log_activity(
            config,
            "ORCH",
            story_id,
            "QA_START",
            &format!("QA review via {}", model),
        )?;

        // Build QA prompt
        let current_story = story_io::parse_story_file(&story.path)?;
        let prompt = prompts::qa_story_prompt(&current_story);
        let vars: HashMap<&str, &str> = HashMap::new();

        if config.dry_run {
            logging::info("[DRY RUN] Would invoke QA agent");
            passed += 1;
            continue;
        }

        invoke_agent(config, "qa", &model, &prompt, &vars)?;

        // AFTER agent returns, read status from disk (bash enforcer pattern)
        let updated = story_io::parse_story_file(&story.path)?;
        let status = updated.frontmatter.status;

        match status {
            StoryStatus::Completed => {
                logging::ok(&format!("QA PASS: {}", story_id));
                logging::log_activity(
                    config,
                    "ORCH",
                    story_id,
                    "QA_PASS",
                    "QA review passed, moving to COMPLETED",
                )?;

                // git_story_commit + push (unless no_commit or dry_run)
                if !no_commit && !config.dry_run {
                    git::git_story_commit(&config.project_root, story_id, title)?;
                } else {
                    logging::info(&format!(
                        "Skipping commit for {} (no_commit/dry_run)",
                        story_id
                    ));
                }
                passed += 1;
            }
            StoryStatus::Refix => {
                logging::err(&format!("QA FAIL: {} sent back to REFIX.", story_id));
                logging::log_activity(
                    config,
                    "ORCH",
                    story_id,
                    "QA_FAIL",
                    "QA review failed, status set to REFIX",
                )?;
                failed += 1;
            }
            other => {
                // Ambiguity protection: force REFIX + qa_status FAIL
                logging::warn(&format!(
                    "Ambiguous QA result: status={}. Forcing REFIX + qa_status=FAIL.",
                    other.label()
                ));
                story_io::update_story_status(&story.path, StoryStatus::Refix)?;
                story_io::update_story_field(&story.path, "qa_status", "FAIL")?;
                logging::log_activity(
                    config,
                    "ORCH",
                    story_id,
                    "QA_FAIL",
                    &format!("Forced REFIX — ambiguous status {}", other.label()),
                )?;
                failed += 1;
            }
        }
    }

    // Summary
    let total = queue.len();
    logging::info(&format!(
        "QA phase complete: {}/{} passed, {} failed.",
        passed, total, failed
    ));
    logging::log_progress(config, &format!("QA: {} passed, {} failed", passed, failed))?;

    Ok(())
}
