use crate::logging;
use bmadder_core::agent::PiDevOutput;
use bmadder_core::config::Config;
use regex::Regex;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// Build a pi Command that loads a skill and processes given input files non-interactively.
pub fn build_pi_command(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
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

    let mut cmd = Command::new(&config.pi_dev.command);
    for arg in &config.pi_dev.args {
        let resolved = arg
            .replace("{model}", model)
            .replace("{skill}", &skill_path.to_string_lossy());
        cmd.arg(resolved);
    }
    for extra in extra_args {
        cmd.arg(extra);
    }
    // Append file references so pi sees them as initial context
    for file in files {
        let path = config.project_root.join(file);
        cmd.arg(format!("@{}", path.display()));
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
    let mut cmd = build_pi_command(config, role_key, model, files, extra_args)?;

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
    use std::path::Path;

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

/// Build a minimal test Config in a temp dir (legacy format still works).
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
codex = "gpt5"
"#;
        let config_path = dir.join("bmadder.toml");
        std::fs::write(&config_path, toml).unwrap();
        bmadder_core::config::Config::load(&config_path).unwrap()
    }
}
