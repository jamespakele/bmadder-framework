use bmadder_core::config::Config;
use crate::logging;
use bmadder_core::agent::PiDevOutput;
use regex::Regex;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// Expand ~ to the user's home directory in a path string.
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, &path[2..]);
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

/// Build a command that loads a skill and processes given input files non-interactively.
/// Supports both pi (@file syntax) and moa-rust (--file syntax) via config.file_arg.
/// `mode` selects phase-specific command/args/file_arg overrides (Plan, QA) when configured.
pub fn build_pi_command(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
    mode: CommandMode,
) -> Result<Command, Box<dyn std::error::Error>> {
    let skill_path = config.resolve_skill_path(role_key).ok_or_else(|| {
        format!(
            "role '{}': skill directory not found at .agent/skills/{}",
            role_key,
            config
                .roles
                .get(role_key)
                .map(|r| r.skill.as_str())
                .unwrap_or("???")
        )
    })?;

    // Select command/args/file_arg — phase-specific override or default
    let (raw_command, args, file_arg) = match mode {
        CommandMode::Plan if !config.pi_dev.plan_command.is_empty() => (
            &config.pi_dev.plan_command,
            &config.pi_dev.plan_args,
            if config.pi_dev.plan_file_arg.is_empty() {
                &config.pi_dev.file_arg
            } else {
                &config.pi_dev.plan_file_arg
            },
        ),
        CommandMode::Qa if !config.pi_dev.qa_command.is_empty() => (
            &config.pi_dev.qa_command,
            &config.pi_dev.qa_args,
            if config.pi_dev.qa_file_arg.is_empty() {
                &config.pi_dev.file_arg
            } else {
                &config.pi_dev.qa_file_arg
            },
        ),
        _ => (
            &config.pi_dev.command,
            &config.pi_dev.args,
            &config.pi_dev.file_arg,
        ),
    };

    // Expand ~ in command path (Rust's Command::new doesn't do shell expansion)
    let command = expand_tilde(raw_command);

    let mut cmd = Command::new(command);
    for arg in args {
        let resolved = arg
            .replace("{model}", model)
            .replace("{skill}", &skill_path.to_string_lossy());
        cmd.arg(resolved);
    }
    for extra in extra_args {
        cmd.arg(extra);
    }
    // Append file references using the configured prefix
    for file in files {
        let path = config.project_root.join(file);
        if file_arg == "@" {
            cmd.arg(format!("@{}", path.display()));
        } else {
            cmd.arg(file_arg);
            cmd.arg(path);
        }
    }
    cmd.current_dir(&config.project_root);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    Ok(cmd)
}

/// Invoke pi with a skill for automatic, non-interactive processing.
/// The skill is loaded via --skill; context files are passed as @ paths.
/// Returns the parsed PiDevOutput (JSON mode) or a constructed AgentResult on fallback.
pub fn invoke_agent(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
) -> Result<PiDevOutput, Box<dyn std::error::Error>> {
    invoke_agent_with(config, role_key, model, files, extra_args, CommandMode::Default)
}

/// Like invoke_agent but uses plan-specific command/args if configured.
pub fn invoke_agent_plan(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
) -> Result<PiDevOutput, Box<dyn std::error::Error>> {
    invoke_agent_with(config, role_key, model, files, extra_args, CommandMode::Plan)
}

/// Like invoke_agent but uses QA-specific command/args if configured
/// (e.g. moa-rust for multi-model QA review).
pub fn invoke_agent_qa(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
) -> Result<PiDevOutput, Box<dyn std::error::Error>> {
    invoke_agent_with(config, role_key, model, files, extra_args, CommandMode::Qa)
}

/// Which pipeline phase's command template to use for an invocation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CommandMode {
    /// Default `[pi_dev]` command/args.
    Default,
    /// Plan-phase override (`plan_command` / `plan_args` / `plan_file_arg`).
    Plan,
    /// QA-phase override (`qa_command` / `qa_args` / `qa_file_arg`).
    Qa,
}


fn invoke_agent_with(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
    mode: CommandMode,
) -> Result<PiDevOutput, Box<dyn std::error::Error>> {
    let mut cmd = build_pi_command(config, role_key, model, files, extra_args, mode)?;

    let output = cmd.spawn()?.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Try parsing JSON output (pi --mode json)
    if let Ok(parsed) = serde_json::from_str::<PiDevOutput>(stdout.trim()) {
        if !parsed.success {
            logging::warn(&format!(
                "pi {} reported failure: {:?}",
                role_key,
                parsed.error.as_deref().unwrap_or("no detail")
            ));
        }
        return Ok(parsed);
    }

    // Fallback: pi may have written JSON to stderr or its output stream
    if let Ok(parsed) = serde_json::from_str::<PiDevOutput>(stderr.trim()) {
        if !parsed.success {
            logging::warn(&format!(
                "pi {} reported failure (stderr): {:?}",
                role_key,
                parsed.error.as_deref().unwrap_or("no detail")
            ));
        }
        return Ok(parsed);
    }

    // Absolute fallback: treat exit status
    if !output.status.success() {
        return Err(format!(
            "pi {} exited {}: {}",
            role_key,
            output.status.code().unwrap_or(-1),
            stderr.trim()
        )
        .into());
    }

    // Success with unparsable output is fine (might be plain text from skill)
    Ok(PiDevOutput {
        success: true,
        error: None,
        output_summary: Some(format!(
            "pi {} completed ({} bytes stdout, {} bytes stderr)",
            role_key,
            stdout.len(),
            stderr.len()
        )),
    })
}

/// State machine for Gemini exponential backoff.
pub struct GeminiBackoff {
    current: Mutex<Duration>,
    initial: Duration,
    max: Duration,
}

impl GeminiBackoff {
    pub fn new(initial_secs: u64, max_secs: u64) -> Self {
        Self {
            current: Mutex::new(Duration::from_secs(initial_secs)),
            initial: Duration::from_secs(initial_secs),
            max: Duration::from_secs(max_secs),
        }
    }

    /// Double the backoff duration, capped at max. Returns the new duration.
    pub fn backoff(&self) -> Duration {
        let mut current = self.current.lock().unwrap();
        *current = (*current * 2).min(self.max);
        *current
    }

    /// Return the current backoff duration without modifying it.
    #[allow(dead_code)]
    pub fn current(&self) -> Duration {
        *self.current.lock().unwrap()
    }

    /// Reset to the initial duration.
    pub fn reset(&self) {
        *self.current.lock().unwrap() = self.initial;
    }
}

/// Check stderr/stdout for Gemini rate-limit signatures.
pub fn is_gemini_rate_limited(stderr: &str, stdout: &str) -> bool {
    let pattern =
        Regex::new(r"(?i)(429|rateLimitExceeded|MODEL_CAPACITY_EXHAUSTED|No capacity available)")
            .unwrap();
    pattern.is_match(stderr) || pattern.is_match(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_detection() {
        assert!(is_gemini_rate_limited("", "429 Too Many Requests"));
        assert!(is_gemini_rate_limited("rateLimitExceeded", ""));
        assert!(is_gemini_rate_limited("MODEL_CAPACITY_EXHAUSTED", ""));
        assert!(is_gemini_rate_limited("no capacity available here", ""));
        assert!(!is_gemini_rate_limited("", "all good"));
    }

    #[test]
    fn test_gemini_backoff() {
        let bo = GeminiBackoff::new(30, 300);
        assert_eq!(bo.current(), Duration::from_secs(30));

        let d = bo.backoff();
        assert_eq!(d, Duration::from_secs(60));
        assert_eq!(bo.current(), Duration::from_secs(60));

        let d = bo.backoff();
        assert_eq!(d, Duration::from_secs(120));

        bo.reset();
        assert_eq!(bo.current(), Duration::from_secs(30));
    }
}

#[cfg(test)]
pub mod utils {
    use std::path::Path;

    pub fn make_test_config(dir: &Path) -> bmadder_core::config::Config {
        let toml = r#"
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

[roles.dev]
personality = "bmad-agent-dev"
model = "gpt5"
skill = "bmad-dev-story"

[roles.qa]
personality = "bmad-agent-dev"
model = "sonnet"
skill = "bmad-code-review"

[agent_hints]
specialist = "kimi27"
generalist = "dsv4pro"
planning-qa = "glm52"
"#;
        let config_path = dir.join("bmadder.toml");
        std::fs::write(&config_path, toml).unwrap();
        bmadder_core::config::Config::load(&config_path).unwrap()
    }
}
