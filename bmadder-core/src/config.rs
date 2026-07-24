use crate::story::Story;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Filesystem paths relative to the project root (bmadder.toml location).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub skills_dir: PathBuf,
    pub stories_dir: PathBuf,
    pub state_dir: PathBuf,
    pub prd_file: PathBuf,
    pub architecture_file: PathBuf,
    #[serde(default = "default_orchestrator_marker")]
    pub orchestrator_marker: PathBuf,
    /// Directory for frozen story specs (default: _bmad/frozen)
    #[serde(default = "default_frozen_dir")]
    pub frozen_dir: PathBuf,
    /// Deferred work ledger file (default: _bmad/deferred-work.md)
    #[serde(default = "default_deferred_work")]
    pub deferred_work_file: PathBuf,
}

fn default_orchestrator_marker() -> PathBuf {
    PathBuf::from("_bmad/orchestrator-master.md")
}
fn default_frozen_dir() -> PathBuf {
    PathBuf::from("_bmad/frozen")
}
fn default_deferred_work() -> PathBuf {
    PathBuf::from("_bmad/deferred-work.md")
}

/// Per-role configuration: which personality, model, and skill to use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub personality: String,
    pub model: String,
    /// BMAD skill directory name under skills_dir (e.g. "bmad-dev-story").
    pub skill: String,
    /// Optional: override the `[pi_dev]` command for this role (e.g.
    /// "~/apps/moa-rust" for multi-model consensus). Empty → pi_dev.command.
    /// When set, this role runs through the override command (the "consensus"
    /// pass); bmadder then runs a `pi` apply pass to write structured
    /// decisions, since moa-rust backends have no file-edit tools.
    #[serde(default)]
    pub command: String,
    /// Optional: override args for this role. Empty → pi_dev.args.
    /// When `command` is set you usually must also set `args` (e.g.
    /// `["run", "--skill", "{skill}"]` for moa-rust).
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional: file_arg for this role. Empty → pi_dev.file_arg.
    #[serde(default)]
    pub file_arg: String,
}

/// Default limits and timing values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default = "default_max_dev_iterations")]
    pub max_dev_iterations: u32,
    #[serde(default = "default_max_sm_iterations")]
    pub max_sm_iterations: u32,
    #[serde(default = "default_max_qa_passes")]
    pub max_qa_passes: u32,
    #[serde(default = "default_story_timeout_seconds")]
    pub story_timeout_seconds: u64,
    #[serde(default = "default_gemini_cooldown_seconds")]
    pub gemini_cooldown_seconds: u64,
    #[serde(default = "default_gemini_initial_backoff")]
    pub gemini_initial_backoff: u64,
}

fn default_max_dev_iterations() -> u32 {
    3
}
fn default_max_sm_iterations() -> u32 {
    5
}
fn default_max_qa_passes() -> u32 {
    3
}
fn default_story_timeout_seconds() -> u64 {
    1800
}
fn default_gemini_cooldown_seconds() -> u64 {
    15
}
fn default_gemini_initial_backoff() -> u64 {
    30
}

/// pi subprocess invocation template.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PiDevConfig {
    #[serde(default = "default_pi_command")]
    pub command: String,
    #[serde(default = "default_pi_args")]
    pub args: Vec<String>,
    /// How to pass context files: "@" for pi (default), "--file" for moa-rust.
    #[serde(default = "default_file_arg")]
    pub file_arg: String,
}

/// Hermes Kanban bridge integration: whether BMADder reports story state to a
/// Hermes Kanban board, which board, and which Hermes install to talk to.
///
/// When `bridge_report = true`, BMADder auto-enables JSONL event emission
/// (`config.jsonl_events = true`) and spawns the Python bridge subprocess so
/// story state is mirrored to Hermes automatically. `hermes_home` points to
/// the Hermes install on disk (e.g. "~/.hermes") so the bridge can locate the
/// `hermes` CLI binary. `rest_url` optionally overrides the REST API endpoint
/// for status updates (default http://127.0.0.1:8000).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesConfig {
    /// When true, BMADder emits JSONL events and spawns the Python bridge
    /// subprocess so story state is mirrored to Hermes automatically.
    #[serde(default)]
    pub bridge_report: bool,
    /// Hermes Kanban board slug (e.g. "bmadder-framework", "ai-r3").
    /// Empty string → derive from the project folder name at load time.
    #[serde(default)]
    pub project_slug: String,
    /// Filesystem path to the Hermes install (where `hermes-agent/` lives).
    /// Default "~/.hermes". Used by the bridge to locate the `hermes` binary
    /// for CLI calls (`<hermes_home>/hermes-agent/venv/bin/hermes`), falling
    /// back to `hermes` on PATH. `~` is expanded at load time.
    #[serde(default = "default_hermes_home")]
    pub hermes_home: String,
    /// Optional REST API URL for status updates (the CLI has no status setter).
    /// Empty → default http://127.0.0.1:8000. Set this for remote installs
    /// (e.g. "https://hermes.example.com" or "http://host.docker.internal:8000").
    #[serde(default)]
    pub rest_url: String,
    /// Optional path to the bridge script. Empty → look in
    /// `<project_root>/scripts/bmadder-kanban-bridge.py`.
    #[serde(default)]
    pub bridge_script: String,
    /// Bridge poll interval in seconds (default 10).
    #[serde(default = "default_bridge_poll")]
    pub bridge_poll_seconds: u64,
}

fn default_bridge_poll() -> u64 {
    10
}

fn default_hermes_home() -> String {
    "~/.hermes".into()
}

impl Default for HermesConfig {
    fn default() -> Self {
        Self {
            bridge_report: false,
            project_slug: String::new(),
            hermes_home: default_hermes_home(),
            rest_url: String::new(),
            bridge_script: String::new(),
            bridge_poll_seconds: default_bridge_poll(),
        }
    }
}

impl HermesConfig {
    /// Expand a leading `~` to the user's home directory.
    fn expand_home(path: &str) -> String {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                return Path::new(&home).join(rest).to_string_lossy().to_string();
            }
        } else if path == "~" {
            if let Some(home) = std::env::var_os("HOME") {
                return home.to_string_lossy().to_string();
            }
        }
        path.to_string()
    }

    /// REST API base URL for status updates (the CLI has no status setter).
    /// Uses `rest_url` if set; otherwise defaults to http://127.0.0.1:8000
    /// (the Hermes gateway's default bind address).
    pub fn rest_base(&self) -> String {
        let url = self.rest_url.trim();
        if url.is_empty() {
            "http://127.0.0.1:8000".to_string()
        } else {
            url.trim_end_matches('/').to_string()
        }
    }

    /// Resolve the `hermes` CLI binary path. Looks for
    /// `<hermes_home>/hermes-agent/venv/bin/hermes` (the standard install
    /// layout), falling back to `hermes` on PATH if not found.
    pub fn hermes_binary(&self) -> String {
        let home = Self::expand_home(&self.hermes_home);
        let candidate = Path::new(&home)
            .join("hermes-agent")
            .join("venv")
            .join("bin")
            .join("hermes");
        if candidate.exists() {
            candidate.to_string_lossy().to_string()
        } else {
            "hermes".to_string()
        }
    }

    /// Resolve the board slug: use `project_slug` if set, else derive from the
    /// project root folder name (kebab-cased).
    pub fn board_slug(&self, project_root: &Path) -> String {
        if !self.project_slug.is_empty() {
            return self.project_slug.clone();
        }
        project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase().replace('_', "-"))
            .unwrap_or_else(|| "bmadder".to_string())
    }

    /// Resolve the bridge script path. If `bridge_script` is set, use it
    /// (relative paths resolved against the project root). Otherwise look in
    /// `<project_root>/scripts/bmadder-kanban-bridge.py`. Returns None if the
    /// resolved path does not exist on disk.
    pub fn bridge_script_path(&self, project_root: &Path) -> Option<PathBuf> {
        let candidate = if self.bridge_script.is_empty() {
            project_root
                .join("scripts")
                .join("bmadder-kanban-bridge.py")
        } else {
            let p = PathBuf::from(&self.bridge_script);
            if p.is_absolute() {
                p
            } else {
                project_root.join(p)
            }
        };
        if candidate.exists() {
            Some(candidate)
        } else {
            None
        }
    }
}

fn default_pi_command() -> String {
    "pi".into()
}

fn default_pi_args() -> Vec<String> {
    vec![
        "--model".into(),
        "{model}".into(),
        "--skill".into(),
        "{skill}".into(),
        "--print".into(),
        "--mode".into(),
        "json".into(),
        "--no-session".into(),
        "--approve".into(),
    ]
}

fn default_file_arg() -> String {
    "@".into()
}

/// Which pipeline phase is being executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Plan,
    Dev,
    QA,
}

/// Top-level configuration, loaded from bmadder.toml + env + CLI.
#[derive(Debug, Clone)]
pub struct Config {
    /// Absolute path to the directory containing bmadder.toml.
    pub project_root: PathBuf,
    /// Resolved absolute paths.
    pub paths: PathsConfig,
    /// Logical model name → pi.dev --model string (e.g., "sonnet" → "claude-sonnet-4").
    pub models: HashMap<String, String>,
    /// Role key → role config.
    pub roles: HashMap<String, RoleConfig>,
    /// agent_hint value → logical model key (e.g., "specialist" → "kimi27").
    pub agent_hints: HashMap<String, String>,
    /// Default limits / timing.
    pub defaults: DefaultsConfig,
    /// pi.dev command template.
    pub pi_dev: PiDevConfig,
    /// Hermes Kanban bridge integration.
    pub hermes: HermesConfig,

    // --- Runtime overrides (applied after TOML load) ---
    /// True when --dry-run is set.
    pub dry_run: bool,
    /// True when --json is set.
    pub json_output: bool,
    /// Force a specific model key for all roles (from --agent or BMADDER_AGENT).
    pub agent_override: Option<String>,
    /// Override story timeout (from --timeout).
    pub timeout_override: Option<u64>,
    /// True when --jsonl-events is set (emit structured events to events.jsonl).
    pub jsonl_events: bool,
}

/// Intermediate TOML representation (before path resolution).
#[derive(Debug, Clone, Deserialize)]
struct ConfigToml {
    #[serde(default)]
    paths: PathsConfigToml,
    #[serde(default)]
    models: HashMap<String, String>,
    #[serde(default)]
    roles: HashMap<String, RoleConfig>,
    #[serde(default)]
    agent_hints: HashMap<String, String>,
    #[serde(default)]
    defaults: DefaultsConfig,
    #[serde(default)]
    pi_dev: PiDevConfig,
    #[serde(default)]
    hermes: HermesConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PathsConfigToml {
    skills_dir: Option<String>,
    stories_dir: Option<String>,
    state_dir: Option<String>,
    prd_file: Option<String>,
    architecture_file: Option<String>,
    orchestrator_marker: Option<String>,
    frozen_dir: Option<String>,
    deferred_work_file: Option<String>,
}

impl Config {
    /// Load config from a bmadder.toml file. All relative paths are resolved
    /// against the parent directory of the config file.
    pub fn load(config_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let project_root = config_path
            .parent()
            .ok_or("config file has no parent directory")?
            .to_path_buf();
        let project_root = project_root.canonicalize().unwrap_or(project_root);

        let content = std::fs::read_to_string(config_path)?;
        let toml: ConfigToml = toml::from_str(&content)?;

        let resolve_path = |rel: Option<&str>, default: &str| -> PathBuf {
            let rel = rel.unwrap_or(default);
            let p = Path::new(rel);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                project_root.join(p)
            }
        };

        let paths = PathsConfig {
            skills_dir: resolve_path(toml.paths.skills_dir.as_deref(), ".agent/skills"),
            stories_dir: resolve_path(toml.paths.stories_dir.as_deref(), "docs/backlog/stories"),
            state_dir: resolve_path(toml.paths.state_dir.as_deref(), "_bmad"),
            prd_file: resolve_path(toml.paths.prd_file.as_deref(), "docs/prd.md"),
            architecture_file: resolve_path(
                toml.paths.architecture_file.as_deref(),
                "docs/architecture.md",
            ),
            orchestrator_marker: resolve_path(
                toml.paths.orchestrator_marker.as_deref(),
                "_bmad/orchestrator-master.md",
            ),
            frozen_dir: resolve_path(toml.paths.frozen_dir.as_deref(), "_bmad/frozen"),
            deferred_work_file: resolve_path(
                toml.paths.deferred_work_file.as_deref(),
                "_bmad/deferred-work.md",
            ),
        };

        let bridge_report = toml.hermes.bridge_report;
        Ok(Config {
            project_root,
            paths,
            models: toml.models,
            roles: toml.roles,
            agent_hints: toml.agent_hints,
            defaults: toml.defaults,
            pi_dev: toml.pi_dev,
            hermes: toml.hermes,
            // Auto-enable JSONL events when Hermes bridge reporting is on,
            // so the Python bridge has structured events to read.
            jsonl_events: bridge_report,
            dry_run: false,
            json_output: false,
            agent_override: None,
            timeout_override: None,
        })
    }

    /// Apply BMADDER_* environment variable overrides.
    pub fn apply_env_overrides(&mut self) {
        if let Ok(agent) = std::env::var("BMADDER_AGENT") {
            self.agent_override = Some(agent);
        }
        // Per-phase env overrides stored as agent_override variants handled
        // during resolve_model at invocation time.
        if let Ok(v) = std::env::var("BMADDER_MAX_ITER") {
            if let Ok(n) = v.parse() {
                self.defaults.max_dev_iterations = n;
            }
        }
        if let Ok(v) = std::env::var("BMADDER_MAX_SM_ITER") {
            if let Ok(n) = v.parse() {
                self.defaults.max_sm_iterations = n;
            }
        }
        if let Ok(v) = std::env::var("BMADDER_MAX_DEV_ITER") {
            if let Ok(n) = v.parse() {
                self.defaults.max_dev_iterations = n;
            }
        }
        if let Ok(v) = std::env::var("BMADDER_STORY_TIMEOUT") {
            if let Ok(n) = v.parse() {
                self.defaults.story_timeout_seconds = n;
            }
        }
    }

    /// Resolve which `pi.dev --model` string to use for a given phase + story.
    /// Priority: --agent CLI > BMADDER_AGENT env > per-phase env > story agent_hint > TOML role default.
    pub fn resolve_model(&self, phase: Phase, story: Option<&Story>) -> String {
        // 1. CLI --agent override
        if let Some(ref agent) = self.agent_override {
            return self.model_key_to_model(agent);
        }

        // 2. Per-phase env override
        let phase_env = match phase {
            Phase::Plan => "BMADDER_PLAN_AGENT",
            Phase::Dev => "BMADDER_DEV_AGENT",
            Phase::QA => "BMADDER_QA_AGENT",
        };
        if let Ok(agent) = std::env::var(phase_env) {
            return self.model_key_to_model(&agent);
        }

        // 3. Story agent_hint (dev phase only)
        if phase == Phase::Dev {
            if let Some(story) = story {
                if let Some(ref hint) = story.frontmatter.agent_hint {
                    if let Some(model_key) = self.agent_hints.get(hint.as_str()) {
                        if let Some(model) = self.models.get(model_key.as_str()) {
                            return model.clone();
                        }
                    }
                }
            }
        }

        // 4. TOML role default
        let role_key = match phase {
            Phase::Plan => "sm",
            Phase::Dev => "dev",
            Phase::QA => "qa",
        };
        self.role_model(role_key)
    }

    /// Build the absolute path to a personality SKILL.md file.
    /// When skill-based invocation is used this is informational; the skill
    /// directory loaded via --skill includes its own SKILL.md.
    pub fn resolve_personality_path(&self, role_key: &str) -> Option<PathBuf> {
        let role = self.roles.get(role_key)?;
        let p = self
            .paths
            .skills_dir
            .join(&role.personality)
            .join("SKILL.md");
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    /// Build the absolute path to a skill directory under skills_dir.
    pub fn resolve_skill_path(&self, role_key: &str) -> Option<PathBuf> {
        let role = self.roles.get(role_key)?;
        let p = self.paths.skills_dir.join(&role.skill);
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    /// Path to the prompt temp file.
    pub fn prompt_tmp_path(&self) -> PathBuf {
        self.paths.state_dir.join(".prompt-tmp.md")
    }

    /// Path to the activity log.
    pub fn activity_log_path(&self) -> PathBuf {
        self.paths.state_dir.join("logs/activity.log")
    }

    /// Path to the progress log.
    pub fn progress_file_path(&self) -> PathBuf {
        self.paths.state_dir.join("progress.txt")
    }

    /// Path to the structured JSONL event log (alongside activity.log).
    pub fn events_jsonl_path(&self) -> PathBuf {
        self.paths.state_dir.join("logs/events.jsonl")
    }

    // --- helpers ---

    fn model_key_to_model(&self, key_or_name: &str) -> String {
        // First try as a logical key in [models]
        if let Some(model) = self.models.get(key_or_name) {
            return model.clone();
        }
        // Then try as a raw model name (for direct use)
        key_or_name.to_string()
    }

    fn role_model(&self, role_key: &str) -> String {
        self.roles
            .get(role_key)
            .and_then(|r| self.models.get(&r.model))
            .cloned()
            .unwrap_or_else(|| {
                self.roles
                    .get(role_key)
                    .map(|r| r.model.clone())
                    .unwrap_or_else(|| "claude-sonnet-4".into())
            })
    }

    /// True when `role_key` has a per-role command override (e.g. moa-rust).
    /// When true, that role's consensus pass runs through the override command
    /// and bmadder follows it with a `pi` apply pass.
    pub fn role_has_command_override(&self, role_key: &str) -> bool {
        self.roles
            .get(role_key)
            .map(|r| !r.command.is_empty())
            .unwrap_or(false)
    }

    /// `"moa-rust"` when the role has a command override, else `"pi"`.
    /// Used in log banners so the user can see which engine handles a role.
    pub fn role_engine_label(&self, role_key: &str) -> &'static str {
        if self.role_has_command_override(role_key) {
            "moa-rust"
        } else {
            "pi"
        }
    }
}

impl DefaultsConfig {
    pub fn new() -> Self {
        Self {
            max_dev_iterations: default_max_dev_iterations(),
            max_sm_iterations: default_max_sm_iterations(),
            max_qa_passes: default_max_qa_passes(),
            story_timeout_seconds: default_story_timeout_seconds(),
            gemini_cooldown_seconds: default_gemini_cooldown_seconds(),
            gemini_initial_backoff: default_gemini_initial_backoff(),
        }
    }
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::{Story, StoryFrontmatter, StoryStatus};
    use std::path::PathBuf;

    fn sample_toml() -> &'static str {
        r#"
[paths]
skills_dir = ".agent/skills"
stories_dir = "docs/backlog/stories"
state_dir = "_bmad"

[models]
sonnet = "claude-sonnet-4"
gpt5 = "gpt-5"
kimi27 = "ollama/kimi-k2.7-code:cloud"
dsv4pro = "ollama/deepseek-v4-pro:cloud"
glm52 = "ollama/glm-5.2:cloud"

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
"#
    }

    #[test]
    fn parse_full_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, sample_toml()).unwrap();

        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.models.get("sonnet").unwrap(), "claude-sonnet-4");
        assert_eq!(config.roles.len(), 3);
        assert_eq!(
            config.roles.get("dev").unwrap().personality,
            "bmad-agent-dev"
        );
    }

    #[test]
    fn parse_minimal_toml_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, "").unwrap();

        let config = Config::load(&config_path).unwrap();
        assert!(config.models.is_empty());
        assert!(config.roles.is_empty());
        assert_eq!(config.defaults.max_dev_iterations, 3);
        assert_eq!(config.defaults.story_timeout_seconds, 1800);
    }

    #[test]
    fn path_resolution_relative_to_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, "[paths]\nskills_dir = \"my-skills\"\n").unwrap();

        let config = Config::load(&config_path).unwrap();
        assert!(config.paths.skills_dir.ends_with("my-skills"));
        assert!(
            config.paths.skills_dir.is_absolute()
                || config.paths.skills_dir.starts_with(dir.path())
        );
    }

    #[test]
    fn resolve_model_dev_with_agent_hint() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, sample_toml()).unwrap();

        let config = Config::load(&config_path).unwrap();

        // Without agent_hint → use role default
        let fm = StoryFrontmatter {
            story_id: "S-1".into(),
            title: "T".into(),
            status: StoryStatus::ReadyForDev,
            epic_id: None,
            priority: None,
            agent_hint: None,
            assigned_dev: None,
            po_alignment: None,
            qa_status: None,
            created_at: None,
            updated_at: None,
            links: vec![],
        };
        let story = Story {
            path: PathBuf::from("s.md"),
            frontmatter: fm.clone(),
            body: String::new(),
        };
        assert_eq!(config.resolve_model(Phase::Dev, Some(&story)), "gpt-5");

        // With agent_hint "specialist" → should resolve to "ollama/kimi-k2.7-code:cloud"
        let fm_specialist = StoryFrontmatter {
            agent_hint: Some("specialist".into()),
            ..fm
        };
        let story_specialist = Story {
            frontmatter: fm_specialist,
            ..story
        };
        assert_eq!(
            config.resolve_model(Phase::Dev, Some(&story_specialist)),
            "ollama/kimi-k2.7-code:cloud"
        );
    }

    #[test]
    fn resolve_personality_and_skill_paths() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, sample_toml()).unwrap();

        // Create the skill dir so resolve_skill_path succeeds
        std::fs::create_dir_all(dir.path().join(".agent/skills/bmad-dev-story")).unwrap();
        std::fs::create_dir_all(dir.path().join(".agent/skills/bmad-agent-dev")).unwrap();
        std::fs::write(
            dir.path().join(".agent/skills/bmad-agent-dev/SKILL.md"),
            "# test",
        )
        .unwrap();

        let config = Config::load(&config_path).unwrap();

        let p = config.resolve_personality_path("dev").unwrap();
        assert!(p.ends_with("bmad-agent-dev/SKILL.md"));

        let s = config.resolve_skill_path("dev").unwrap();
        assert!(s.ends_with("bmad-dev-story"));
    }
    #[test]
    fn parse_role_command_override() {
        // SM uses moa-rust; PO is left as a single-model pi role. This is the
        // decoupled case the per-role command design enables.
        let toml = r#"
[pi_dev]
command = "pi"
args = ["--model","{model}","--skill","{skill}"]
file_arg = "@"

[roles.sm]
personality = "bmad-agent-dev"
model = "glm52"
skill = "bmad-create-epics-and-stories"
command = "~/apps/moa-rust"
args = ["run","--skill","{skill}"]
file_arg = "--file"

[roles.po]
personality = "bmad-agent-dev"
model = "minim3"
skill = "bmad-create-epics-and-stories"
"#;
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, toml).unwrap();

        let config = Config::load(&config_path).unwrap();
        // SM override is captured per-role
        let sm = config.roles.get("sm").unwrap();
        assert_eq!(sm.command, "~/apps/moa-rust");
        assert_eq!(sm.args, vec!["run", "--skill", "{skill}"]);
        assert_eq!(sm.file_arg, "--file");
        assert!(config.role_has_command_override("sm"));
        assert_eq!(config.role_engine_label("sm"), "moa-rust");
        // PO has no override → single-model pi
        let po = config.roles.get("po").unwrap();
        assert!(po.command.is_empty());
        assert!(!config.role_has_command_override("po"));
        assert_eq!(config.role_engine_label("po"), "pi");
        // pi_dev defaults remain intact
        assert_eq!(config.pi_dev.command, "pi");
        assert_eq!(config.pi_dev.file_arg, "@");
    }

    #[test]
    fn role_command_defaults_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, "").unwrap();
        let config = Config::load(&config_path).unwrap();
        // No roles → no override, label is "pi"
        assert!(!config.role_has_command_override("qa"));
        assert_eq!(config.role_engine_label("qa"), "pi");
    }

    #[test]
    fn hermes_config_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, "").unwrap();
        let config = Config::load(&config_path).unwrap();
        assert!(!config.hermes.bridge_report);
        assert_eq!(config.hermes.hermes_home, "~/.hermes");
        assert_eq!(config.hermes.rest_base(), "http://127.0.0.1:8000");
        assert!(!config.jsonl_events);
    }

    #[test]
    fn hermes_config_bridge_report_enables_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(
            &config_path,
            "[hermes]\nbridge_report = true\nproject_slug = \"ai-r3\"\n",
        )
        .unwrap();
        let config = Config::load(&config_path).unwrap();
        assert!(config.hermes.bridge_report);
        assert_eq!(config.hermes.project_slug, "ai-r3");
        assert!(config.jsonl_events);
        assert_eq!(config.hermes.board_slug(dir.path()), "ai-r3");
    }

    #[test]
    fn hermes_config_board_slug_derives_from_folder() {
        let dir = tempfile::tempdir_in(".").unwrap();
        let config_path = dir.path().join("bmadder.toml");
        std::fs::write(&config_path, "[hermes]\nbridge_report = true\n").unwrap();
        let config = Config::load(&config_path).unwrap();
        let slug = config.hermes.board_slug(dir.path());
        assert!(!slug.is_empty());
        assert!(!slug.contains('_'));
    }

    #[test]
    fn hermes_config_rest_url_override() {
        let cfg = HermesConfig {
            rest_url: "https://hermes.example.com/".into(),
            ..Default::default()
        };
        assert_eq!(cfg.rest_base(), "https://hermes.example.com");
    }

    #[test]
    fn hermes_config_rest_url_empty_defaults_local() {
        let cfg = HermesConfig::default();
        assert_eq!(cfg.rest_base(), "http://127.0.0.1:8000");
    }

    #[test]
    fn hermes_config_hermes_binary_falls_back_to_path() {
        // A nonexistent hermes_home → falls back to "hermes" on PATH.
        let cfg = HermesConfig {
            hermes_home: "/nonexistent/hermes".into(),
            ..Default::default()
        };
        assert_eq!(cfg.hermes_binary(), "hermes");
    }
}
