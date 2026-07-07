use crate::agent::invoke_agent;
use crate::logging;
use bmadder_core::config::Config;

/// Quick-fix fast path: skip SM/PO, invoke bmad-quick-dev skill directly.
///
/// This is for bug fixes, tweaks, and small features that don't need
/// the full SM→PO→DEV→QA ceremony. The bmad-quick-dev skill handles:
/// - Routing (one-shot for trivial work, full plan→implement→review otherwise)
/// - Spec creation with frozen intent
/// - Implementation
/// - Adversarial review (3 reviewers)
/// - Commit + present
pub fn run_quick_fix(config: &Config, description: &str) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Quick Fix (bmad-quick-dev)");

    if description.trim().is_empty() {
        return Err("No fix description provided. Usage: bmadder quick-fix <description>".into());
    }

    let model = config
        .models
        .get(
            config
                .roles
                .get("quick")
                .map(|r| r.model.as_str())
                .unwrap_or("kimi27"),
        )
        .cloned()
        .unwrap_or_else(|| "ollama/kimi-k2.7-code:cloud".into());
    logging::info(&format!("Quick-fix model: {}", model));
    logging::info(&format!("Intent: {}", description));
    logging::log_activity(
        config,
        "ORCH",
        "-",
        "QUICK_FIX_START",
        &format!("bmad-quick-dev via {}", model),
    )?;

    // The bmad-quick-dev skill handles its own routing, planning, implementation, and review.
    // We pass the description as the system prompt and context files as needed.
    let system_prompt = format!(
        r#"Implement the following request using the bmad-quick-dev workflow.

Request: {description}

Pipeline rules:
- Follow the bmad-quick-dev skill workflow exactly (step files, checkpoints, etc.)
- Route appropriately: one-shot for trivial changes, full plan→implement→review for anything else
- Write spec files to the implementation artifacts directory
- Update sprint-status.yaml if it exists
- Append deferred work to _bmad/deferred-work.md if anything is deferred
- Commit when done (conventional commit message from the spec title)
- Log to _bmad/logs/activity.log
"#,
        description = description,
    );

    // Context files: PRD, architecture, progress, existing stories
    let mut files = Vec::new();
    if config.paths.prd_file.exists() {
        files.push(
            config
                .paths
                .prd_file
                .strip_prefix(&config.project_root)
                .unwrap_or(&config.paths.prd_file)
                .to_string_lossy()
                .to_string(),
        );
    }
    if config.paths.architecture_file.exists() {
        files.push(
            config
                .paths
                .architecture_file
                .strip_prefix(&config.project_root)
                .unwrap_or(&config.paths.architecture_file)
                .to_string_lossy()
                .to_string(),
        );
    }
    let progress = config.progress_file_path();
    if progress.exists() {
        files.push(
            progress
                .strip_prefix(&config.project_root)
                .unwrap_or(&progress)
                .to_string_lossy()
                .to_string(),
        );
    }
    let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();

    if config.dry_run {
        logging::info("[DRY RUN] Would invoke bmad-quick-dev");
        logging::info(&format!("Description: {}", description));
        return Ok(());
    }

    let result = invoke_agent(
        config,
        "quick",
        &model,
        &file_refs,
        &["--system-prompt", &system_prompt],
    )?;

    logging::info(&format!(
        "Quick-fix result: success={} summary={:?}",
        result.success, result.output_summary
    ));
    logging::log_activity(
        config,
        "ORCH",
        "-",
        "QUICK_FIX_DONE",
        &format!(
            "success={}, summary={:?}",
            result.success, result.output_summary
        ),
    )?;

    if result.success {
        logging::ok("Quick fix completed.");
    } else {
        logging::err("Quick-fix reported failure. Check logs for details.");
        if let Some(ref err) = result.error {
            logging::err(err);
        }
    }

    Ok(())
}
