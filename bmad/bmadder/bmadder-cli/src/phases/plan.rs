use crate::agent::invoke_agent;
use crate::logging;
use crate::prompts;
use crate::story_io;
use bmadder_core::config::Config;
use bmadder_core::story::StoryStatus;
use std::collections::HashMap;

pub fn run_plan(
    config: &Config,
    skip_sm: bool,
    skip_po: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Phase: PLAN (SM -> Stories -> PO Gate)");

    // Precondition checks
    if !config.paths.prd_file.exists() {
        return Err("docs/prd.md missing.".into());
    }
    if !config.paths.architecture_file.exists() {
        return Err("docs/architecture.md missing.".into());
    }

    // Step 1: Scrum Master (batch)
    if skip_sm {
        logging::warn("Skipping SM (--skip-sm). Using existing stories.");
        logging::log_activity(
            config,
            "ORCH",
            "-",
            "SM_SKIP",
            "SM skipped, using existing stories",
        )?;
        let drafts = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Draft);
        if drafts == 0 {
            return Err("No DRAFT stories found. Run without --skip-sm first.".into());
        }
        logging::info(&format!("{} existing DRAFT stories found.", drafts));
    } else {
        let model = config.resolve_model(bmadder_core::config::Phase::Plan, None);
        logging::info(&format!("Step 1/2: Scrum Master [{}]", model));
        logging::log_activity(
            config,
            "ORCH",
            "-",
            "SM_START",
            &format!("SM sharding via {}", model),
        )?;

        let prompt = prompts::sm_batch_prompt(true);
        let vars: HashMap<&str, &str> = HashMap::new();

        if config.dry_run {
            logging::info("[DRY RUN] Would invoke SM with pi.dev");
        } else {
            invoke_agent(config, "sm", &model, &prompt, &vars)?;
        }
        logging::log_activity(config, "SM", "-", "SM_DONE", "Sharding complete")?;
        logging::ok("SM sharding complete.");

        // Validate stories
        let errors = story_io::validate_stories(&config.paths.stories_dir)?;
        if !errors.is_empty() {
            for e in &errors {
                logging::err(e);
            }
        } else {
            logging::ok("All stories valid.");
        }
        let drafts = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Draft);
        logging::info(&format!("{} DRAFT stories created.", drafts));
    }

    // Step 2: Product Owner (batch)
    if skip_po {
        logging::warn("Skipping PO gate (--skip-po). Auto-approving all DRAFTs.");
        let drafts =
            story_io::get_stories_by_status(&config.paths.stories_dir, StoryStatus::Draft)?;
        for story in drafts {
            story_io::update_story_field(&story.path, "status", "READY_FOR_DEV")?;
            story_io::update_story_field(&story.path, "po_alignment", "APPROVED")?;
        }
        logging::log_activity(config, "ORCH", "-", "PO_SKIP", "All drafts auto-approved")?;
    } else {
        let model = config.resolve_model(bmadder_core::config::Phase::Plan, None);
        logging::info(&format!("Step 2/2: Product Owner [{}]", model));
        logging::log_activity(
            config,
            "ORCH",
            "-",
            "PO_START",
            &format!("PO review via {}", model),
        )?;

        let prompt = prompts::po_batch_prompt();
        let vars: HashMap<&str, &str> = HashMap::new();

        if config.dry_run {
            logging::info("[DRY RUN] Would invoke PO with pi.dev");
        } else {
            invoke_agent(config, "po", &model, &prompt, &vars)?;
        }
        logging::log_activity(config, "PO", "-", "PO_DONE", "Review complete")?;
        logging::ok("PO review complete.");
    }

    let ready = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::ReadyForDev);
    let revise = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Revise);
    logging::info(&format!(
        "Result: {} READY_FOR_DEV, {} REVISE",
        ready, revise
    ));
    logging::log_progress(
        config,
        &format!("PLAN: {} approved, {} need revision", ready, revise),
    )?;

    Ok(())
}
