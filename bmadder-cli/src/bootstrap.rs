use crate::git;
use crate::logging;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn run_bootstrap(project_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("BMADder Bootstrap");

    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    logging::info(&format!("Project directory: {}", project_dir.display()));

    // Step 1: Create folder structure
    logging::info("Step 1/7: Creating folder structure...");
    let dirs = ["docs/backlog/stories", "docs/standards", "_bmad/logs"];
    for d in &dirs {
        let p = project_dir.join(d);
        fs::create_dir_all(&p)?;
        logging::ok(&format!("  Created: {}", d));
    }

    // Step 2: Generate _bmad/orchestrator-master.md marker file
    logging::info("Step 2/7: Generating orchestrator marker...");
    let marker_path = project_dir.join("_bmad/orchestrator-master.md");
    let marker_content = r#"# BMADder Orchestrator Master

This directory is managed by the BMADder orchestration pipeline.
Do not edit files here manually unless you know what you're doing.

## State Directory
- `progress.txt` — pipeline progress log
- `logs/activity.log` — detailed activity log
- `.prompt-tmp.md` — temporary prompt file (ephemeral)
"#;
    fs::write(&marker_path, marker_content)?;
    logging::ok("  Created: _bmad/orchestrator-master.md");

    // Step 3: Generate bmadder.toml (default template)
    logging::info("Step 3/6: Configuring bmadder.toml...");
    let config_path = project_dir.join("bmadder.toml");

    if config_path.exists() {
        logging::info("  bmadder.toml already exists. Skipping generation.");
    } else {
        let default_config = r#"# BMADder Orchestrator Configuration
# See docs/standards/ for configuration reference.

[paths]
skills_dir = ".agent/skills"
stories_dir = "docs/backlog/stories"
state_dir = "_bmad"

[models]
sonnet = "claude-sonnet-4"
gpt5 = "gpt-5"

[roles.sm]
personality = "bmad-agent-dev"
model = "sonnet"
skill = "bmad-create-epics-and-stories"

[roles.po]
personality = "bmad-agent-dev"
model = "sonnet"
skill = "bmad-create-epics-and-stories"

[roles.dev]
personality = "bmad-agent-dev"
model = "gpt5"
skill = "bmad-dev-story"

[roles.qa]
personality = "bmad-agent-dev"
model = "sonnet"
skill = "bmad-code-review"

[agent_hints]
codex = "gpt5"
claude = "sonnet"

[defaults]
max_dev_iterations = 10
max_sm_iterations = 5
max_qa_passes = 3
story_timeout_seconds = 1800
gemini_cooldown_seconds = 15
gemini_initial_backoff = 30

[pi_dev]
command = "pi"
args = [
    "--model", "{model}",
    "--skill", "{skill}",
    "--print",
    "--mode", "json",
    "--no-session",
    "--approve",
]
"#;
        fs::write(&config_path, default_config)?;
        logging::ok("  Created: bmadder.toml");
    }

    // Add .gitignore entries
    logging::info("  Updating .gitignore...");
    let gitignore_path = project_dir.join(".gitignore");
    let entries = ["_bmad/.prompt-tmp.md", "_bmad/logs/"];
    let mut existing = String::new();
    if gitignore_path.exists() {
        existing = fs::read_to_string(&gitignore_path)?;
    }
    let mut added = false;
    for entry in &entries {
        if !existing.lines().any(|l| l.trim() == *entry) {
            if !existing.is_empty() && !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(entry);
            existing.push('\n');
            added = true;
        }
    }
    if added {
        fs::write(&gitignore_path, &existing)?;
        logging::ok("  Updated .gitignore with BMADder entries.");
    } else {
        logging::info("  .gitignore already contains BMADder entries.");
    }

    // Generate .mise.toml if missing
    let mise_path = project_dir.join(".mise.toml");
    if mise_path.exists() {
        logging::info("  .mise.toml already exists. Skipping.");
    } else {
        let mise_content = r#"[tools]
# Managed by BMADder — tool version pins
"#;
        fs::write(&mise_path, mise_content)?;
        logging::ok("  Created: .mise.toml");
    }

    // Step 4: Tooling check
    logging::info("Step 4/6: Checking tooling...");

    // pi --version
    match Command::new("pi").arg("--version").output() {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            logging::ok(&format!("  pi: {}", version));
        }
        Err(e) => {
            logging::warn(&format!(
                "  pi not found: {}. Install before running pipeline.",
                e
            ));
        }
    }

    // git --version
    match Command::new("git").arg("--version").output() {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            logging::ok(&format!("  git: {}", version));
        }
        Err(e) => {
            logging::warn(&format!("  git not found: {}. Please install git.", e));
        }
    }

    // Step 5: Git init if needed
    logging::info("Step 5/6: Git initialization...");
    let initialized = git::git_init_if_needed(&project_dir)?;
    if initialized {
        logging::ok("  Git repository initialized.");
    } else {
        logging::info("  Git repository already exists.");
    }

    // Step 6: Project files check
    logging::info("Step 6/6: Checking project files...");
    let prd_path = project_dir.join("docs/prd.md");
    let arch_path = project_dir.join("docs/architecture.md");

    if prd_path.exists() && prd_path.metadata().map(|m| m.len() > 500).unwrap_or(false) {
        logging::ok("  docs/prd.md: OK (>500 bytes)");
    } else if prd_path.exists() {
        logging::warn("  docs/prd.md exists but is <500 bytes. Please flesh it out.");
    } else {
        logging::warn("  docs/prd.md not found. Create it before running the pipeline.");
    }

    if arch_path.exists() && arch_path.metadata().map(|m| m.len() > 500).unwrap_or(false) {
        logging::ok("  docs/architecture.md: OK (>500 bytes)");
    } else if arch_path.exists() {
        logging::warn("  docs/architecture.md exists but is <500 bytes. Please flesh it out.");
    } else {
        logging::warn("  docs/architecture.md not found. Create it before running the pipeline.");
    }

    logging::ok("Bootstrap complete!");
    logging::ok(&format!("Project root: {}", project_dir.display()));
    logging::info("Next steps:");
    logging::info("  1. Ensure docs/prd.md and docs/architecture.md are fleshed out.");
    logging::info("  2. Run: bmadder plan");
    logging::info("  3. Run: bmadder cycle");

    Ok(())
}
