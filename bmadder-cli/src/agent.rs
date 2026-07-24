use crate::logging;
use bmadder_core::agent::PiDevOutput;
use bmadder_core::config::Config;
use regex::Regex;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

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
/// `mode` selects per-role command/args/file_arg overrides when configured.
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

    // Select command/args/file_arg — per-role override (e.g. moa-rust) or
    // pi_dev defaults. CommandMode::Default forces pi_dev regardless of any
    // role override (used by moa apply passes that must run pi).
    let role = config.roles.get(role_key);
    let (raw_command, args, file_arg): (&str, &Vec<String>, &str) = match mode {
        CommandMode::Role if role.map_or(false, |r| !r.command.is_empty()) => {
            let r = role.unwrap();
            (
                &r.command,
                if r.args.is_empty() {
                    &config.pi_dev.args
                } else {
                    &r.args
                },
                if r.file_arg.is_empty() {
                    &config.pi_dev.file_arg
                } else {
                    &r.file_arg
                },
            )
        }
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

    // Put pi in its own process group so a timeout kill can reach any
    // children (network clients, subagents) it spawned.
    #[cfg(unix)]
    cmd.process_group(0);

    Ok(cmd)
}

/// Invoke a role's agent for automatic, non-interactive processing.
/// The skill is loaded via --skill; context files are passed as @ paths.
/// Returns the parsed PiDevOutput (JSON mode) or a constructed AgentResult on fallback.
///
/// Honors a per-role `command` override (e.g. moa-rust) when configured;
/// otherwise uses the `[pi_dev]` defaults. This is the consensus pass.
pub fn invoke_agent(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
) -> Result<PiDevOutput, Box<dyn std::error::Error>> {
    invoke_agent_with(
        config,
        role_key,
        model,
        files,
        extra_args,
        CommandMode::Role,
    )
}

/// Like `invoke_agent` but always uses the `[pi_dev]` defaults, ignoring any
/// per-role command override. Used by moa apply passes that must run `pi`
/// even when the role's consensus command is moa-rust (moa-rust backends have
/// no file-edit tools, so the structured decision is applied by pi).
pub fn invoke_agent_default(
    config: &Config,
    role_key: &str,
    model: &str,
    files: &[&str],
    extra_args: &[&str],
) -> Result<PiDevOutput, Box<dyn std::error::Error>> {
    invoke_agent_with(
        config,
        role_key,
        model,
        files,
        extra_args,
        CommandMode::Default,
    )
}

/// Error returned when an agent subprocess is killed because it exceeded
/// the configured `story_timeout_seconds`.
#[derive(Debug)]
pub struct AgentTimeoutError {
    pub role_key: String,
    pub timeout_secs: u64,
    pub pid: i32,
}

impl std::fmt::Display for AgentTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "pi {} timed out after {}s (killed process group {})",
            self.role_key, self.timeout_secs, self.pid
        )
    }
}

impl std::error::Error for AgentTimeoutError {}

/// Returns true if the error is an `AgentTimeoutError`.
pub fn is_agent_timeout(err: &(dyn std::error::Error + 'static)) -> bool {
    err.downcast_ref::<AgentTimeoutError>().is_some()
}

/// Which command template to use for an invocation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CommandMode {
    /// Honor a per-role `command` override when present; else `[pi_dev]` defaults.
    Role,
    /// Always use `[pi_dev]` defaults (moa apply passes).
    Default,
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

    let timeout_secs = config.defaults.story_timeout_seconds;
    let child = cmd.spawn()?;
    let child_pid = child.id() as i32;

    // Watchdog: kill the entire process group if the child runs past timeout.
    let killed = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    let _cancel_tx = if timeout_secs > 0 {
        let killed_clone = killed.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(
            move || match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    unsafe { libc::kill(-child_pid, libc::SIGKILL) };
                    killed_clone.store(true, Ordering::SeqCst);
                }
            },
        );
        Some(tx)
    } else {
        None
    };

    let output = child.wait_with_output()?;

    #[cfg(unix)]
    drop(_cancel_tx);

    if timeout_secs > 0 && killed.load(Ordering::SeqCst) {
        return Err(Box::new(AgentTimeoutError {
            role_key: role_key.to_string(),
            timeout_secs,
            pid: child_pid,
        }));
    }

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

    #[cfg(unix)]
    fn make_timeout_test_config(
        dir: &std::path::Path,
        timeout_secs: u64,
        delay_secs: u64,
    ) -> Config {
        let skills_dir = dir.join(".agent/skills/bmad-dev-story");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let toml = format!(
            r#"
[paths]
skills_dir = ".agent/skills"
stories_dir = "docs/backlog/stories"
state_dir = "_bmad"

[defaults]
story_timeout_seconds = {}

[pi_dev]
command = "sleep"
args = ["{}"]
file_arg = "@"

[models]
gpt5 = "gpt-5"

[roles.dev]
personality = "bmad-agent-dev"
model = "gpt5"
skill = "bmad-dev-story"
"#,
            timeout_secs, delay_secs
        );
        let config_path = dir.join("bmadder.toml");
        std::fs::write(&config_path, toml).unwrap();
        Config::load(&config_path).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn test_agent_timeout_kills_slow_child() {
        let tmp = tempfile::tempdir().unwrap();
        let config = make_timeout_test_config(tmp.path(), 1, 10);

        let start = std::time::Instant::now();
        let err = invoke_agent(&config, "dev", "gpt5", &[], &[])
            .expect_err("slow child should have timed out");

        assert!(
            is_agent_timeout(err.as_ref()),
            "expected AgentTimeoutError, got: {}",
            err
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "timeout should fire quickly, took {:?}",
            start.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_agent_timeout_allows_fast_child() {
        let tmp = tempfile::tempdir().unwrap();
        let config = make_timeout_test_config(tmp.path(), 1, 0);

        let result = invoke_agent(&config, "dev", "gpt5", &[], &[]);
        assert!(
            result.is_ok(),
            "fast child should not time out: {:?}",
            result
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_agent_timeout_zero_disables_watchdog() {
        let tmp = tempfile::tempdir().unwrap();
        let config = make_timeout_test_config(tmp.path(), 0, 1);

        let start = std::time::Instant::now();
        let result = invoke_agent(&config, "dev", "gpt5", &[], &[]);
        assert!(
            result.is_ok(),
            "timeout=0 should disable watchdog: {:?}",
            result
        );
        assert!(
            start.elapsed() >= Duration::from_secs(1),
            "child should have run for ~1s, took {:?}",
            start.elapsed()
        );
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
