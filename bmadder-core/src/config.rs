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
}

fn default_orchestrator_marker() -> PathBuf {
    PathBuf::from("_bmad/orchestrator-master.md")
}

/// Per-role configuration: which personality, model, and skill to use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub personality: String,
    pub model: String,
    /// BMAD skill directory name under skills_dir (e.g. "bmad-dev-story").
    pub skill: String,
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
    10
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

    // --- Runtime overrides (applied after TOML load) ---
    /// True when --dry-run is set.
    pub dry_run: bool,
    /// True when --json is set.
    pub json_output: bool,
    /// Force a specific model key for all roles (from --agent or BMADDER_AGENT).
    pub agent_override: Option<String>,
    /// Override story timeout (from --timeout).
    pub timeout_override: Option<u64>,
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
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PathsConfigToml {
    skills_dir: Option<String>,
    stories_dir: Option<String>,
    state_dir: Option<String>,
    prd_file: Option<String>,
    architecture_file: Option<String>,
    orchestrator_marker: Option<String>,
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
        };

        Ok(Config {
            project_root,
            paths,
            models: toml.models,
            roles: toml.roles,
            agent_hints: toml.agent_hints,
            defaults: toml.defaults,
            pi_dev: toml.pi_dev,
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
        assert_eq!(config.defaults.max_dev_iterations, 10);
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
}
