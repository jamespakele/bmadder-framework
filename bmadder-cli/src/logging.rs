use bmadder_core::config::Config;
use bmadder_core::story::StoryStatus;
use chrono::Utc;
use colored::Colorize;
use serde::Serialize;
use std::io::Write;

/// A structured event written to `_bmad/logs/events.jsonl` when `--jsonl-events` is enabled.
/// This is the machine-readable counterpart to the human-readable activity.log line.
#[derive(Debug, Clone, Serialize)]
pub struct StoryEvent {
    /// ISO 8601 timestamp (UTC).
    pub ts: String,
    /// Who produced the event (e.g. "ORCH", "SM", "DEV", "QA").
    pub actor: String,
    /// Story id the event pertains to (may be empty for run-level events).
    pub story_id: String,
    /// Event type (e.g. "STATUS_CHANGE", "QA_PASS", "QA_FAIL", "COMMIT", "ALL_DONE").
    pub event: String,
    /// Previous status (None for non-transition events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// New status (None for non-transition events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Free-text detail (role/model info, commit hash, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl StoryEvent {
    /// Build a status-transition event.
    pub fn status_change(actor: &str, story_id: &str, from: &str, to: &str, detail: &str) -> Self {
        Self {
            ts: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            actor: actor.to_string(),
            story_id: story_id.to_string(),
            event: "STATUS_CHANGE".to_string(),
            from: Some(from.to_string()),
            to: Some(to.to_string()),
            detail: Some(detail.to_string()),
        }
    }

    /// Build a generic (non-transition) event.
    pub fn simple(actor: &str, story_id: &str, event: &str, detail: &str) -> Self {
        Self {
            ts: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            actor: actor.to_string(),
            story_id: story_id.to_string(),
            event: event.to_string(),
            from: None,
            to: None,
            detail: Some(detail.to_string()),
        }
    }
}

/// Append a structured JSONL event to `_bmad/logs/events.jsonl`.
///
/// Fire-and-forget: write failures are logged to stderr and never block the caller.
/// No-op when `config.jsonl_events` is false.
pub fn log_event(config: &Config, event: &StoryEvent) {
    if !config.jsonl_events {
        return;
    }
    let path = config.events_jsonl_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!(
                "warn: jsonl-events: could not create {}: {}",
                parent.display(),
                e
            );
            return;
        }
    }
    let mut line = match serde_json::to_string(event) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("warn: jsonl-events: serialize error: {}", e);
            return;
        }
    };
    line.push('\n');
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(line.as_bytes()) {
                eprintln!("warn: jsonl-events: write error: {}", e);
            }
        }
        Err(e) => eprintln!(
            "warn: jsonl-events: open error on {}: {}",
            path.display(),
            e
        ),
    }
}

/// Append a line to the activity log.
pub fn log_activity(
    config: &Config,
    actor: &str,
    story_id: &str,
    event: &str,
    detail: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config.activity_log_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = format!(
        "{} | {} | {} | {} | {}\n",
        timestamp, actor, story_id, event, detail
    );
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    f.write_all(line.as_bytes())?;
    // Also emit a structured JSONL event when --jsonl-events is enabled.
    log_event(config, &StoryEvent::simple(actor, story_id, event, detail));
    Ok(())
}

/// Append a line to the progress log.
pub fn log_progress(config: &Config, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = config.progress_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = format!("{} | {}\n", timestamp, message);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    f.write_all(line.as_bytes())?;
    Ok(())
}

/// Write a standout START or END marker to progress.txt.
/// Format: *** START_<key> - <timestamp> ***
pub fn log_marker(
    config: &Config,
    kind: &str,
    key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config.progress_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = format!("*** {}_{} - {} ***\n", kind, key, timestamp);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    f.write_all(line.as_bytes())?;
    Ok(())
}

/// Return true if `*** END_<key> ***` is already in progress.txt (phase completed).
pub fn progress_marker_done(config: &Config, key: &str) -> bool {
    let path = config.progress_file_path();
    if !path.exists() {
        return false;
    }
    let needle = format!("*** END_{}", key);
    std::fs::read_to_string(&path)
        .map(|s| s.contains(&needle))
        .unwrap_or(false)
}

// --- Console output helpers ---

pub fn info(msg: &str) {
    println!("{}  {}", "[INFO]".blue().bold(), msg);
}

pub fn ok(msg: &str) {
    println!("{}    {}", "[OK]".green().bold(), msg);
}

pub fn warn(msg: &str) {
    println!("{}  {}", "[WARN]".yellow().bold(), msg);
}

pub fn err(msg: &str) {
    println!("{}   {}", "[ERR]".red().bold(), msg);
}

pub fn phase_banner(msg: &str) {
    println!("\n{}", msg.cyan().bold());
}

pub fn story_banner(msg: &str) {
    println!();
    let line = "═".repeat(58);
    println!("{}", line.cyan());
    println!("{}  {}", "║".cyan(), msg.cyan());
    println!("{}", line.cyan());
}

/// Print the full status table: story counts, key file checks, agent routing.
pub fn show_status(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("{}", "=".repeat(51).cyan());
    println!("{}", "  BMADder Status".cyan());
    println!("{}", "=".repeat(51).cyan());
    println!();

    let stories_dir = &config.paths.stories_dir;

    let statuses = StoryStatus::all();
    for status in &statuses {
        let count = crate::story_io::count_by_status(stories_dir, *status);
        let label = format!("{: <15}", status.label());
        match status {
            StoryStatus::Completed => println!("  {} {}", label.green(), count),
            StoryStatus::ReadyForDev | StoryStatus::InDev => {
                println!("  {} {}", label.blue(), count)
            }
            StoryStatus::Refix => println!("  {} {}", label.red(), count),
            _ => println!("  {} {}", label.yellow(), count),
        }
    }

    let total: usize = statuses
        .iter()
        .map(|s| crate::story_io::count_by_status(stories_dir, *s))
        .sum();
    println!("\n  Total: {}", total);
    println!();

    println!("{}", "  Key Files:".cyan());
    print_file_check(config, &config.paths.prd_file, "docs/prd.md");
    print_file_check(
        config,
        &config.paths.architecture_file,
        "docs/architecture.md",
    );
    print_file_check(
        config,
        &config.paths.orchestrator_marker,
        "_bmad/orchestrator-master.md",
    );
    print_file_check(config, &config.progress_file_path(), "_bmad/progress.txt");
    println!();

    println!("{}", "  Agent Routing:".cyan());
    let plan_model = config.resolve_model(bmadder_core::config::Phase::Plan, None);
    let dev_model = config.resolve_model(bmadder_core::config::Phase::Dev, None);
    let qa_model = config.resolve_model(bmadder_core::config::Phase::QA, None);
    println!(
        "  plan -> {}    dev -> {}    qa -> {}",
        plan_model, dev_model, qa_model
    );

    if config.agent_override.is_some() {
        warn(&format!(
            "[!] Global override: {}",
            config.agent_override.as_deref().unwrap_or("")
        ));
    }
    println!();

    Ok(())
}

fn print_file_check(config: &Config, path: &std::path::Path, label: &str) {
    let project_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        config.project_root.join(path)
    };
    if project_path.exists()
        && project_path
            .metadata()
            .map(|m| m.len() > 0)
            .unwrap_or(false)
    {
        println!("  {} {}", "[OK]".green(), label);
    } else {
        println!("  {}  {}", "[X]".red(), label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_activity_and_progress() {
        let dir = tempfile::tempdir().unwrap();
        let config = crate::agent::utils::make_test_config(dir.path());

        log_activity(&config, "TEST", "S-1", "TEST_START", "testing").unwrap();
        log_progress(&config, "test message").unwrap();

        let activity = std::fs::read_to_string(config.activity_log_path()).unwrap();
        assert!(activity.contains("TEST_START"));
        assert!(activity.contains("testing"));

        let progress = std::fs::read_to_string(config.progress_file_path()).unwrap();
        assert!(progress.contains("test message"));
    }

    #[test]
    fn test_console_output_smoke() {
        // Just ensure these don't panic
        info("info message");
        ok("ok message");
        warn("warn message");
        err("err message");
        phase_banner("Phase: TEST");
        story_banner("STORY-0001: Test Story");
    }

    #[test]
    fn test_show_status_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = crate::agent::utils::make_test_config(dir.path());
        // create stories dir so it doesn't error
        std::fs::create_dir_all(&config.paths.stories_dir).unwrap();
        show_status(&config).unwrap();
    }
}
