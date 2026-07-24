use crate::agent::{invoke_agent, is_agent_timeout, is_gemini_rate_limited, GeminiBackoff};
use crate::git;
use crate::logging;
use crate::prompts;
use crate::spec;
use crate::story_io;
use bmadder_core::config::{Config, Phase};
use bmadder_core::story::StoryStatus;

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

        logging::log_marker(config, "START", &format!("DEV:{}", story_id))?;

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
        // Circuit breaker: track which verification checks keep failing.
        // If the same checks fail 3 times in a row, advance to PENDING_QA
        // anyway — the dev agent can't fix a template/format problem by re-running.
        let mut repeated_fail_count = 0u32;
        let mut last_failures: Vec<String> = Vec::new();

        while iterations < max_iters {
            iterations += 1;
            logging::info(&format!("--- Iteration {}/{} ---", iterations, max_iters));

            // Build dev invocation
            let current_story = story_io::parse_story_file(&story.path)?;
            let prompt = prompts::dev_story_prompt(&current_story);
            let files: Vec<String> = prompts::dev_story_files(config, &current_story);
            let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();

            if config.dry_run {
                logging::info("[DRY RUN] Would invoke dev agent");
                story_done = true;
                break;
            }

            let result = match invoke_agent(
                config,
                "dev",
                &model,
                &file_refs,
                &["--system-prompt", &prompt],
            ) {
                Ok(r) => r,
                Err(e) => {
                    if is_agent_timeout(e.as_ref()) {
                        logging::err(&format!("DEV timeout for {}: {}", story_id, e));
                        logging::log_activity(
                            config,
                            "ORCH",
                            story_id,
                            "DEV_TIMEOUT",
                            &format!("timed out after {}s", config.defaults.story_timeout_seconds),
                        )?;
                        story_io::update_story_status(&story.path, StoryStatus::Refix)?;
                        break;
                    }
                    return Err(e);
                }
            };

            // Read status from disk after agent returns
            let updated = story_io::parse_story_file(&story.path)?;
            let status = updated.frontmatter.status;

            match status {
                StoryStatus::PendingQA | StoryStatus::Completed => {
                    // Verify against frozen spec
                    let frozen = spec::read_frozen_spec(
                        &config.paths.frozen_dir,
                        &story.frontmatter.story_id,
                    );
                    let verification = spec::verify_after_dev(&updated, frozen.as_deref());
                    if verification.passed {
                        logging::ok(&format!("Story {} verified ✓", story_id));
                        logging::log_activity(
                            config,
                            "ORCH",
                            story_id,
                            "VERIFY_PASS",
                            "Spec verification passed",
                        )?;
                    } else {
                        let failures: Vec<&str> = verification
                            .failures()
                            .iter()
                            .map(|c| c.name.as_str())
                            .collect();
                        logging::warn(&format!(
                            "Story {} verification failed: {:?}",
                            story_id, failures
                        ));
                        logging::log_activity(
                            config,
                            "ORCH",
                            story_id,
                            "VERIFY_FAIL",
                            &format!("Checks failed: {}", failures.join(", ")),
                        )?;

                        // Circuit breaker: if the same checks fail repeatedly,
                        // the dev agent can't fix it (likely a template/format
                        // mismatch). Advance to PENDING_QA and let QA handle it.
                        let current_failures: Vec<String> = verification
                            .failures()
                            .iter()
                            .map(|c| c.name.clone())
                            .collect();
                        if current_failures == last_failures {
                            repeated_fail_count += 1;
                        } else {
                            repeated_fail_count = 1;
                            last_failures = current_failures.clone();
                        }
                        if repeated_fail_count >= 3 {
                            logging::warn(&format!(
                                "Story {} failed same checks {} times — advancing to PENDING_QA (circuit breaker)",
                                story_id, repeated_fail_count
                            ));
                            logging::log_activity(
                                config,
                                "ORCH",
                                story_id,
                                "CIRCUIT_BREAKER",
                                &format!(
                                    "Same checks failed {}x — advancing to PENDING_QA",
                                    repeated_fail_count
                                ),
                            )?;
                            story_io::update_story_status(&story.path, StoryStatus::PendingQA)?;
                            story_done = true;
                            break;
                        }
                        // Don't advance — keep in IN_DEV for another iteration
                        story_io::update_story_status(&story.path, StoryStatus::InDev)?;
                        continue;
                    }
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
                    let stderr = result.error.as_deref().unwrap_or("");
                    let stdout = result.output_summary.as_deref().unwrap_or("");
                    if is_gemini_rate_limited(stderr, stdout) && iterations < max_iters {
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
            logging::log_marker(config, "END", &format!("DEV:{}", story_id))?;
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
