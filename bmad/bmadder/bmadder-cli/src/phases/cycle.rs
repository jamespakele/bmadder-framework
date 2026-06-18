use crate::logging;
use crate::phases::{dev, plan, qa};
use crate::story_io;
use bmadder_core::config::Config;
use bmadder_core::story::StoryStatus;

pub fn run_cycle(
    config: &Config,
    skip_sm: bool,
    skip_po: bool,
    no_commit: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Phase: CYCLE (Plan → Dev → QA loop)");

    // Check if there are stories ready for dev or needing refix
    let ready = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::ReadyForDev);
    let refix = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Refix);

    let needs_plan = ready == 0 && refix == 0;

    if needs_plan {
        // Smart SM skip: if DRAFTs exist and no REVISE, skip SM
        let drafts = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Draft);
        let revise = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Revise);

        let sm_skip = skip_sm || (drafts > 0 && revise == 0);
        let effective_skip_sm = sm_skip;
        let effective_skip_po = skip_po;

        logging::info("No READY_FOR_DEV or REFIX stories. Running plan phase...");
        plan::run_plan(config, effective_skip_sm, effective_skip_po)?;
    } else {
        logging::info(&format!(
            "{} READY_FOR_DEV, {} REFIX stories — skipping plan.",
            ready, refix
        ));
    }

    let max_passes = config.defaults.max_qa_passes;

    for pass in 1..=max_passes {
        logging::info(&format!("--- Cycle pass {}/{} ---", pass, max_passes));

        // Run dev phase
        dev::run_dev(config, None)?;

        // Run QA phase
        qa::run_qa(config, None, no_commit)?;

        // Count REFIX after QA
        let refix_after = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Refix);

        // If no new REFIX stories (refix count didn't go up), we're done
        if refix_after == 0 {
            logging::ok(&format!(
                "Cycle complete after pass {}: 0 REFIX stories remaining.",
                pass
            ));
            break;
        }

        if pass < max_passes {
            logging::info(&format!(
                "{} REFIX stories remain. Continuing to pass {}...",
                refix_after,
                pass + 1
            ));
        } else {
            logging::warn(&format!(
                "Max QA passes ({}) reached with {} REFIX stories still pending.",
                max_passes, refix_after
            ));
        }

        logging::log_progress(
            config,
            &format!("CYCLE pass {}: {} REFIX remaining", pass, refix_after),
        )?;
    }

    // Final: show_status
    logging::show_status(config)?;

    // ALL COMPLETED banner or partial report
    let completed = story_io::count_by_status(&config.paths.stories_dir, StoryStatus::Completed);
    let total = story_io::list_stories(&config.paths.stories_dir)?.len();

    if completed == total && total > 0 {
        logging::ok(&format!(
            "🎉 ALL STORIES COMPLETED! ({}/{})",
            completed, total
        ));
        logging::log_progress(config, "CYCLE: all stories completed")?;
    } else {
        logging::info(&format!(
            "Cycle finished: {}/{} stories completed.",
            completed, total
        ));
        logging::log_progress(
            config,
            &format!("CYCLE: {} completed, {} total", completed, total),
        )?;
    }

    Ok(())
}
