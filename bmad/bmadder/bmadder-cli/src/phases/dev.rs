use crate::agent::{invoke_agent, is_gemini_rate_limited, GeminiBackoff};
use crate::git;
use crate::logging;
use crate::prompts;
use crate::story_io;
use bmadder_core::config::{Config, Phase};
use bmadder_core::story::StoryStatus;
use std::collections::HashMap;

pub fn run_dev(
    config: &Config,
    target_story: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Phase: DEV (Implementation)");

    // Pre-dev snapshot
    if !config.dry_run {
        git::git_snapshot(&config.project_root)?;
    } else {
        logging::info("[DRY RUN] Would take pre-dev git snapshot");
    }

    // Build queue: READY_FOR_DEV (sorted by filename) + REFIX appended
    let mut ready =
        story_io::get_stories_by_status(&config.paths.stories_dir, StoryStatus::ReadyForDev)?;
    ready.sort_by(|a, b| a.path.cmp(&b.path));

    let refix = story_io::get_stories_by_status(&config.paths.stories_dir, StoryStatus::Refix)?;
    let mut queue = ready;
    queue.extend(refix);

    // Filter by target_story if set
    if let Some(target) = target_story {
        let target = target.trim();
        if !target.is_empty() {
            queue = story_io::filter_stories_by_id(queue, target);
        }
    }

    if queue.is_empty() {
        logging::warn("No stories queued: no READY_FOR_DEV or REFIX stories found.");
        logging::log_progress(config, "DEV: nothing to do")?;
        return Ok(());
    }

    logging::info(&format!("{} story/stories queued for dev.", queue.len()));

    let max_iters = config.defaults.max_dev_iterations;
    let gemini_backoff = GeminiBackoff::new(
        config.defaults.gemini_initial_backoff,
        config.defaults.gemini_initial_backoff.saturating_mul(10),
    );

    let mut completed = 0usize;
    let mut stalled = 0usize;

    for story in &queue {
        let story_id = &story.frontmatter.story_id;
        let title = &story.frontmatter.title;
        logging::story_banner(&format!("{}: {}", story_id, title));

        // Resolve agent: check agent_hint → override model
        let model = config.resolve_model(Phase::Dev, Some(story));
        logging::info(&format!("Agent model: {}", model));

        // Reset Gemini backoff per story
        gemini_backoff.reset();

        // Set status IN_DEV
        story_io::update_story_status(&story.path, StoryStatus::InDev)?;
        logging::log_activity(
            config,
            "ORCH",
            story_id,
            "IN_DEV",
            &format!("dev via {}", model),
        )?;

        let mut story_done = false;
        let mut iterations = 0u32;

        while iterations < max_iters {
            iterations += 1;
            logging::info(&format!("--- Iteration {}/{} ---", iterations, max_iters));

            // Build dev prompt
            let current_story = story_io::parse_story_file(&story.path)?;
            let prompt = prompts::dev_story_prompt(&current_story);
            let vars: HashMap<&str, &str> = HashMap::new();

            if config.dry_run {
                logging::info("[DRY RUN] Would invoke dev agent");
                story_done = true;
                break;
            }

            let result = invoke_agent(config, "dev", &model, &prompt, &vars)?;

            // Read status from disk after agent returns
            let updated = story_io::parse_story_file(&story.path)?;
            let status = updated.frontmatter.status;

            match status {
                StoryStatus::PendingQA | StoryStatus::Completed => {
                    logging::ok(&format!("Story {} moved to {}", story_id, status.label()));
                    logging::log_activity(
                        config,
                        "ORCH",
                        story_id,
                        "DEV_DONE",
                        &format!("{} after {} iterations", status.label(), iterations),
                    )?;
                    story_done = true;
                    break;
                }
                _ => {
                    // Check for Gemini rate limiting and apply cooldown
                    if is_gemini_rate_limited(&result.stderr, &result.stdout)
                        && iterations < max_iters
                    {
                        let cooldown = gemini_backoff.backoff();
                        logging::warn(&format!(
                            "Gemini rate limit detected. Cooling down {:?}...",
                            cooldown
                        ));
                        std::thread::sleep(cooldown);
                    }
                }
            }
        }

        if story_done {
            completed += 1;
        } else {
            stalled += 1;
            logging::err(&format!(
                "STALLED: {} did not reach PENDING_QA or COMPLETED after {} iterations.",
                story_id, max_iters
            ));
            logging::log_activity(
                config,
                "ORCH",
                story_id,
                "STALLED",
                &format!("max_dev_iterations={}", max_iters),
            )?;
        }
    }

    // Summary
    let total = queue.len();
    logging::info(&format!(
        "DEV phase complete: {}/{} stories done, {} stalled.",
        completed, total, stalled
    ));
    logging::log_progress(
        config,
        &format!("DEV: {} done, {} stalled", completed, stalled),
    )?;

    Ok(())
}
