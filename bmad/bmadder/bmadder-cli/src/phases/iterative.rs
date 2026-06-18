use crate::agent::invoke_agent;
use crate::git;
use crate::logging;
use crate::prompts;
use crate::story_io;
use bmadder_core::config::{Config, Phase};
use bmadder_core::story::{Story, StoryStatus};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn run_iterative(
    config: &Config,
    from_existing: bool,
    target_story: Option<&str>,
    start_from: Option<&str>,
    skip_po: bool,
    no_commit: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Phase: ITERATIVE (end-to-end pipeline)");

    // Validate prd + arch exist
    if !config.paths.prd_file.exists() {
        return Err("docs/prd.md missing. Cannot run iterative pipeline.".into());
    }
    if !config.paths.architecture_file.exists() {
        return Err("docs/architecture.md missing. Cannot run iterative pipeline.".into());
    }

    // Pre-pipeline snapshot
    if !config.dry_run {
        git::git_snapshot(&config.project_root)?;
    } else {
        logging::info("[DRY RUN] Would take pre-pipeline git snapshot");
    }

    let mut completed = 0usize;
    let mut stalled = 0usize;
    let mut total = 0usize;

    // Step 1: Resume in-flight stories
    let in_flight_statuses: &[StoryStatus] = if from_existing {
        &[StoryStatus::ReadyForDev, StoryStatus::Refix]
    } else {
        &[
            StoryStatus::Draft,
            StoryStatus::Revise,
            StoryStatus::ReadyForDev,
            StoryStatus::Refix,
            StoryStatus::InDev,
            StoryStatus::PendingQA,
        ]
    };

    logging::info("Step 1: Resuming in-flight stories...");

    let mut in_flight: Vec<Story> = Vec::new();
    for status in in_flight_statuses {
        let mut stories = story_io::get_stories_by_status(&config.paths.stories_dir, *status)?;
        stories.sort_by(|a, b| a.path.cmp(&b.path));
        in_flight.extend(stories);
    }

    // Filter by target/start_from
    if let Some(target) = target_story {
        let target = target.trim();
        if !target.is_empty() {
            in_flight = story_io::filter_stories_by_id(in_flight, target);
        }
    }
    if let Some(sf) = start_from {
        let sf = sf.trim();
        if !sf.is_empty() {
            in_flight = story_io::filter_stories_from_id(in_flight, sf);
        }
    }

    if !in_flight.is_empty() {
        logging::info(&format!(
            "Found {} in-flight stories to process.",
            in_flight.len()
        ));
        for story in &in_flight {
            total += 1;
            match process_one_story(config, story, skip_po, no_commit) {
                Ok(true) => completed += 1,
                Ok(false) => stalled += 1,
                Err(e) => {
                    stalled += 1;
                    logging::err(&format!(
                        "Error processing {}: {}",
                        story.frontmatter.story_id, e
                    ));
                }
            }
        }
    } else {
        logging::info("No in-flight stories found.");
    }

    // Step 2: SM-driven loop (create new stories)
    logging::info("Step 2: SM-driven iterative loop...");

    let mut iterations = 0u32;
    let max_iterations = 100u32;

    while iterations < max_iterations {
        iterations += 1;
        logging::info(&format!(
            "--- SM iteration {}/{} ---",
            iterations, max_iterations
        ));

        // Check if ALL_DONE
        if check_all_done(config)? {
            logging::ok("ALL_DONE detected in progress.txt. Pipeline complete!");
            logging::log_progress(config, "ITERATIVE: ALL_DONE — pipeline complete")?;
            break;
        }

        // Create next story
        match sm_create_next_story(config) {
            Ok(Some(new_story_path)) => {
                total += 1;
                logging::info(&format!("New story created: {}", new_story_path.display()));

                let story = story_io::parse_story_file(&new_story_path)?;
                match process_one_story(config, &story, skip_po, no_commit) {
                    Ok(true) => completed += 1,
                    Ok(false) => stalled += 1,
                    Err(e) => {
                        stalled += 1;
                        logging::err(&format!(
                            "Error processing {}: {}",
                            story.frontmatter.story_id, e
                        ));
                    }
                }
            }
            Ok(None) => {
                logging::info("SM did not create a new story. Checking ALL_DONE...");
                // Re-check ALL_DONE after SM finishes without creating
                if check_all_done(config)? {
                    logging::ok("ALL_DONE detected. Pipeline complete!");
                    break;
                }
                // If not ALL_DONE and no new story, might be a stall
                logging::warn(
                    "SM returned without new story but ALL_DONE not detected. Retrying...",
                );
            }
            Err(e) => {
                logging::err(&format!("SM create next story failed: {}", e));
                break;
            }
        }
    }

    if iterations >= max_iterations {
        logging::warn(&format!("Reached max SM iterations ({})", max_iterations));
    }

    // Final report
    logging::info(&format!(
        "Iterative pipeline complete: {}/{} completed, {} stalled.",
        completed, total, stalled
    ));
    logging::log_progress(
        config,
        &format!(
            "ITERATIVE: {} completed, {} stalled, {} total",
            completed, stalled, total
        ),
    )?;

    Ok(())
}

/// Process one story through its pipeline phases.
/// Returns Ok(true) if completed, Ok(false) if stalled.
fn process_one_story(
    config: &Config,
    story: &Story,
    skip_po: bool,
    no_commit: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let story_id = &story.frontmatter.story_id;
    let title = &story.frontmatter.title;
    logging::story_banner(&format!("{}: {}", story_id, title));

    // Read current status
    let current = story_io::parse_story_file(&story.path)?;
    let status = current.frontmatter.status;

    match status {
        StoryStatus::Completed => {
            logging::info(&format!("{} already COMPLETED. Skipping.", story_id));
            return Ok(true);
        }
        StoryStatus::Draft | StoryStatus::Revise => {
            // Phase 1: SM↔PO loop
            process_sm_po_loop(config, story, skip_po)?;
            // Re-read status
            let updated = story_io::parse_story_file(&story.path)?;
            let new_status = updated.frontmatter.status;

            match new_status {
                StoryStatus::ReadyForDev
                | StoryStatus::Refix
                | StoryStatus::InDev
                | StoryStatus::PendingQA => {
                    // Fall through to Phase 2
                }
                _ => {
                    logging::err(&format!(
                        "{} SM↔PO loop ended with unexpected status: {}. Stalled.",
                        story_id,
                        new_status.label()
                    ));
                    return Ok(false);
                }
            }
        }
        _ => {
            // READY_FOR_DEV, REFIX, IN_DEV, PENDING_QA → go straight to Phase 2
        }
    }

    // Phase 2: Dev↔QA loop
    let result = process_dev_qa_loop(config, story, no_commit)?;

    if result {
        logging::ok(&format!("{} completed successfully.", story_id));
        logging::log_progress(config, &format!("{}: completed", story_id))?;
    } else {
        logging::err(&format!("{} stalled in Dev↔QA loop.", story_id));
    }

    Ok(result)
}

/// SM↔PO loop for story refinement (DRAFT/REVISE phase).
fn process_sm_po_loop(
    config: &Config,
    story: &Story,
    skip_po: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let max_sm_iters = config.defaults.max_sm_iterations;
    let model = config.resolve_model(Phase::Plan, None);

    logging::info(&format!(
        "Starting SM↔PO loop for {} (max {} iterations)...",
        story.frontmatter.story_id, max_sm_iters
    ));

    for iter in 1..=max_sm_iters {
        let current = story_io::parse_story_file(&story.path)?;
        let status = current.frontmatter.status;

        // If already READY_FOR_DEV, exit loop
        if status == StoryStatus::ReadyForDev {
            logging::ok(&format!(
                "{} now READY_FOR_DEV. Exiting SM↔PO loop.",
                story.frontmatter.story_id
            ));
            break;
        }

        // If not DRAFT or REVISE, shouldn't be here, but handle gracefully
        if status != StoryStatus::Draft && status != StoryStatus::Revise {
            break;
        }

        logging::info(&format!("SM↔PO iteration {}/{}", iter, max_sm_iters));

        // Step A: SM write/revise
        logging::info("SM: writing/revising story...");
        let sm_prompt = prompts::sm_write_story_prompt(&current);
        let vars: HashMap<&str, &str> = HashMap::new();

        if config.dry_run {
            logging::info("[DRY RUN] Would invoke SM");
        } else {
            invoke_agent(config, "sm", &model, &sm_prompt, &vars)?;
        }

        // Step B: PO review (unless skip_po)
        let updated = story_io::parse_story_file(&story.path)?;
        if skip_po {
            logging::warn("Skipping PO (--skip-po). Auto-approving.");
            story_io::update_story_status(&story.path, StoryStatus::ReadyForDev)?;
            story_io::update_story_field(&story.path, "po_alignment", "APPROVED")?;
            logging::log_activity(
                config,
                "ORCH",
                &story.frontmatter.story_id,
                "PO_SKIP",
                "Auto-approved, status → READY_FOR_DEV",
            )?;
            break;
        }

        logging::info("PO: reviewing...");
        let po_prompt = prompts::po_single_prompt(&updated);
        if config.dry_run {
            logging::info("[DRY RUN] Would invoke PO");
            break;
        }
        invoke_agent(config, "po", &model, &po_prompt, &vars)?;

        // Check status after PO review
        let after_po = story_io::parse_story_file(&story.path)?;
        if after_po.frontmatter.status == StoryStatus::ReadyForDev {
            logging::ok("PO approved. Story is READY_FOR_DEV.");
            break;
        }

        if iter == max_sm_iters {
            logging::err(&format!(
                "{} reached max SM iterations ({}) without PO approval.",
                story.frontmatter.story_id, max_sm_iters
            ));
            logging::log_activity(
                config,
                "ORCH",
                &story.frontmatter.story_id,
                "SM_PO_STALLED",
                &format!("max_sm_iterations={}", max_sm_iters),
            )?;
        }
    }

    Ok(())
}

/// Dev↔QA loop for implementation and review.
/// Returns true if the story passed QA, false if stalled.
fn process_dev_qa_loop(
    config: &Config,
    story: &Story,
    no_commit: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let max_dev_iters = config.defaults.max_dev_iterations;
    let dev_model = config.resolve_model(Phase::Dev, Some(story));
    let qa_model = config.resolve_model(Phase::QA, None);

    logging::info(&format!(
        "Starting Dev↔QA loop for {} (max {} dev iterations)...",
        story.frontmatter.story_id, max_dev_iters
    ));

    for iter in 1..=max_dev_iters {
        let current = story_io::parse_story_file(&story.path)?;
        let status = current.frontmatter.status;

        // Already completed: done
        if status == StoryStatus::Completed {
            if !no_commit && !config.dry_run {
                git::git_story_commit(
                    &config.project_root,
                    &story.frontmatter.story_id,
                    &story.frontmatter.title,
                )?;
            }
            return Ok(true);
        }

        // Already pending QA → skip dev, go to QA
        if status != StoryStatus::PendingQA {
            // --- Dev sub-loop ---
            logging::info(&format!(
                "Dev {} iteration {}/{} [{}]",
                story.frontmatter.story_id, iter, max_dev_iters, dev_model
            ));

            if status != StoryStatus::InDev {
                story_io::update_story_status(&story.path, StoryStatus::InDev)?;
            }

            let dev_prompt = prompts::dev_story_prompt(&current);
            let vars: HashMap<&str, &str> = HashMap::new();

            if config.dry_run {
                logging::info("[DRY RUN] Would invoke dev agent");
            } else {
                invoke_agent(config, "dev", &dev_model, &dev_prompt, &vars)?;
            }
        }

        // --- QA review ---
        let after_dev = story_io::parse_story_file(&story.path)?;
        let after_dev_status = after_dev.frontmatter.status;

        if after_dev_status == StoryStatus::Completed {
            if !no_commit && !config.dry_run {
                git::git_story_commit(
                    &config.project_root,
                    &story.frontmatter.story_id,
                    &story.frontmatter.title,
                )?;
            }
            return Ok(true);
        }

        if after_dev_status != StoryStatus::PendingQA {
            // Force to PENDING_QA for QA review
            story_io::update_story_status(&story.path, StoryStatus::PendingQA)?;
        }

        logging::info(&format!(
            "QA review for {} [{}]",
            story.frontmatter.story_id, qa_model
        ));

        let qa_prompt = prompts::qa_story_prompt(&after_dev);
        let vars: HashMap<&str, &str> = HashMap::new();

        if config.dry_run {
            logging::info("[DRY RUN] Would invoke QA agent");
            return Ok(true);
        }

        invoke_agent(config, "qa", &qa_model, &qa_prompt, &vars)?;

        // Check QA result
        let after_qa = story_io::parse_story_file(&story.path)?;
        match after_qa.frontmatter.status {
            StoryStatus::Completed => {
                logging::ok(&format!("QA PASS: {}", story.frontmatter.story_id));
                if !no_commit && !config.dry_run {
                    git::git_story_commit(
                        &config.project_root,
                        &story.frontmatter.story_id,
                        &story.frontmatter.title,
                    )?;
                }
                logging::log_activity(
                    config,
                    "ORCH",
                    &story.frontmatter.story_id,
                    "QA_PASS",
                    &format!("passed after {} dev iterations", iter),
                )?;
                return Ok(true);
            }
            StoryStatus::Refix => {
                logging::warn(&format!(
                    "QA sent {} back to REFIX. Looping dev...",
                    story.frontmatter.story_id
                ));
                logging::log_activity(
                    config,
                    "ORCH",
                    &story.frontmatter.story_id,
                    "QA_FAIL",
                    &format!("REFIX on iteration {}", iter),
                )?;
                // Continue to next dev iteration
            }
            other => {
                logging::warn(&format!(
                    "Ambiguous QA result for {}: status={}. Forcing REFIX.",
                    story.frontmatter.story_id,
                    other.label()
                ));
                story_io::update_story_status(&story.path, StoryStatus::Refix)?;
                story_io::update_story_field(&story.path, "qa_status", "FAIL")?;
            }
        }
    }

    // Max dev iterations reached without passing QA
    logging::err(&format!(
        "{} stalled: max_dev_iterations ({}) reached without QA PASS.",
        story.frontmatter.story_id, max_dev_iters
    ));
    logging::log_activity(
        config,
        "ORCH",
        &story.frontmatter.story_id,
        "STALLED",
        &format!("max_dev_iterations={}", max_dev_iters),
    )?;

    Ok(false)
}

/// Check if "ALL_DONE" appears in progress.txt content.
fn check_all_done(config: &Config) -> Result<bool, Box<dyn std::error::Error>> {
    let progress_path = config.progress_file_path();
    if !progress_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&progress_path)?;
    Ok(content.contains("ALL_DONE"))
}

/// Invoke SM with sm_single_prompt. Detect new story file by comparing
/// before/after story listings. Returns the path of the new story, or None.
fn sm_create_next_story(config: &Config) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let model = config.resolve_model(Phase::Plan, None);

    // List stories before invoking SM
    let before = story_io::list_stories(&config.paths.stories_dir)?;

    logging::info(&format!("SM: creating next story [{}]...", model));
    logging::log_activity(
        config,
        "ORCH",
        "-",
        "SM_NEXT",
        "Creating next story from PRD",
    )?;

    let prompt = prompts::sm_single_prompt();
    let vars: HashMap<&str, &str> = HashMap::new();

    if config.dry_run {
        logging::info("[DRY RUN] Would invoke SM for next story");
        return Ok(None);
    }

    invoke_agent(config, "sm", &model, &prompt, &vars)?;

    // Check for ALL_DONE after SM returns
    if check_all_done(config)? {
        logging::ok("SM signaled ALL_DONE.");
        return Ok(None);
    }

    // List stories after, detect new file
    let after = story_io::list_stories(&config.paths.stories_dir)?;
    match story_io::detect_new_story_file(&before, &after) {
        Some(new_path) => {
            logging::ok(&format!("New story detected: {}", new_path.display()));
            logging::log_activity(
                config,
                "SM",
                "-",
                "STORY_CREATED",
                &format!("Created {}", new_path.display()),
            )?;
            Ok(Some(new_path))
        }
        None => {
            logging::warn("SM did not create a new story file.");
            Ok(None)
        }
    }
}
