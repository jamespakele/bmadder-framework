use bmadder_core::agent::AgentResult;
use bmadder_core::config::Config;
use regex::Regex;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// Build the prompt temp file with variable substitution.
pub fn build_prompt_file(
    config: &Config,
    template: &str,
    variables: &HashMap<&str, &str>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut prompt = template.to_string();
    for (key, val) in variables {
        prompt = prompt.replace(&format!("{{{}}}", key), val);
    }
    let prompt_path = config.prompt_tmp_path();
    // Ensure parent dir exists
    if let Some(parent) = prompt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&prompt_path)?;
    f.write_all(prompt.as_bytes())?;
    Ok(prompt_path)
}

/// Build a pi.dev Command from config template for a given role + prompt file.
pub fn build_pi_dev_command(
    config: &Config,
    role_key: &str,
    model: &str,
    prompt_file: &Path,
) -> Result<Command, Box<dyn std::error::Error>> {
    let personality = config
        .resolve_personality_path(role_key)
        .ok_or_else(|| format!("role '{}' not found in config", role_key))?;
    let headless = config
        .resolve_headless_path(role_key)
        .ok_or_else(|| format!("role '{}' not found in config", role_key))?;

    let mut cmd = Command::new(&config.pi_dev.command);
    for arg in &config.pi_dev.args {
        let resolved = arg
            .replace("{model}", model)
            .replace("{personality}", &personality.to_string_lossy())
            .replace("{headless}", &headless.to_string_lossy())
            .replace("{prompt_file}", &prompt_file.to_string_lossy())
            .replace("{workspace}", &config.project_root.to_string_lossy())
            .replace(
                "{timeout}",
                &config.defaults.story_timeout_seconds.to_string(),
            );
        cmd.arg(resolved);
    }
    cmd.current_dir(&config.project_root);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    Ok(cmd)
}

/// Invoke pi.dev sub-agent with a given prompt.
pub fn invoke_agent(
    config: &Config,
    role_key: &str,
    model: &str,
    prompt: &str,
    variables: &HashMap<&str, &str>,
) -> Result<AgentResult, Box<dyn std::error::Error>> {
    let prompt_file = build_prompt_file(config, prompt, variables)?;
    let mut cmd = build_pi_dev_command(config, role_key, model, &prompt_file)?;

    let output = cmd.spawn()?.wait_with_output()?;

    // Try parsing JSON output first
    if let Ok(pi_dev) = serde_json::from_slice::<bmadder_core::agent::PiDevOutput>(&output.stdout) {
        return Ok(AgentResult::from_pi_dev(pi_dev));
    }

    // Fallback: check exit code
    Ok(AgentResult {
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        timed_out: false,
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

    #[test]
    fn test_prompt_variable_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let config = crate::agent::utils::make_test_config(dir.path());

        let template = "Story: {story_id}\nFile: @{story_file}\n";
        let vars: HashMap<&str, &str> = [
            ("story_id", "STORY-0001"),
            ("story_file", "docs/backlog/stories/story-0001.md"),
        ]
        .iter()
        .cloned()
        .collect();

        let path = build_prompt_file(&config, template, &vars).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("STORY-0001"));
        assert!(content.contains("story-0001.md"));
    }

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

/// Build a minimal test Config in a temp dir.
pub mod utils {

    use std::path::Path;

    pub fn make_test_config(dir: &Path) -> bmadder_core::config::Config {
        let toml = r#"
[paths]
skills_dir = ".agent/skills"
headless_dir = "scripts/headless-skills"
stories_dir = "docs/backlog/stories"
state_dir = "_bmad"

[models]
sonnet = "claude-sonnet-4"
gpt5 = "gpt-5"

[roles.sm]
personality = "bmad-agent-dev"
model = "sonnet"
headless = "sm-create-stories.md"

[roles.dev]
personality = "bmad-agent-dev"
model = "gpt5"
headless = "dev-story.md"

[roles.qa]
personality = "bmad-agent-dev"
model = "sonnet"
headless = "qa-review.md"

[agent_hints]
codex = "gpt5"
"#;
        let config_path = dir.join("bmadder.toml");
        std::fs::write(&config_path, toml).unwrap();
        bmadder_core::config::Config::load(&config_path).unwrap()
    }
}
