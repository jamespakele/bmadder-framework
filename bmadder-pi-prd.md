# BMADder-pi PRD — Rust Orchestrator for the BMADder Framework with `pi.dev` Sub-Agent Integration

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Solution Overview](#2-solution-overview)
3. [Config-Driven Architecture (Zero Recompile)](#3-config-driven-architecture-zero-recompile)
4. [Project Structure Contract](#4-project-structure-contract)
5. [Story File Format & State Machine](#5-story-file-format--state-machine)
6. [Agent Routing & Sub-Agent Invocation](#6-agent-routing--sub-agent-invocation)
7. [Phase Implementations](#7-phase-implementations)
8. [Pipeline Modes: Batch vs Iterative](#8-pipeline-modes-batch-vs-iterative)
9. [Git Integration & Codebase Safety](#9-git-integration--codebase-safety)
10. [Crash Recovery & Resume](#10-crash-recovery--resume)
11. [Logging, Activity Tracking & Status Display](#11-logging-activity-tracking--status-display)
12. [Bootstrap Module](#12-bootstrap-module)
13. [Auth Preflight & Billing Safety](#13-auth-preflight--billing-safety)
14. [CLI Surface](#14-cli-surface)
15. [Environment Variable Overlay](#15-environment-variable-overlay)
16. [Crate Structure](#16-crate-structure)
17. [Key Dependencies](#17-key-dependencies)
18. [Testing & Validation Strategy](#18-testing--validation-strategy)
19. [Migration Path](#19-migration-path)
20. [What Changes vs What Stays the Same](#20-what-changes-vs-what-stays-the-same)
21. [Appendix A: Complete `bmadder.toml` Reference](#appendix-a-complete-bmaddertoml-reference)
22. [Appendix B: Headless Skill Reference](#appendix-b-headless-skill-reference)
23. [Appendix C: BMAD Agent Personality Reference](#appendix-c-bmad-agent-personality-reference)

---

## 1. Problem Statement

The current BMADder orchestrator is implemented in Bash (~1,000 lines across `bmadder.sh` and `bmadder-iterative.sh`) plus supporting Python scripts (`bootstrap_bmadder.py`, `preflight_auth.py`, `validate_stories.py`, `sync_headless_skills.py`). It works, but it has several limitations:

1. **YAML frontmatter parsing via `sed`/`grep` is fragile.** Story files use YAML frontmatter for their state machine. The bash scripts parse this with line-oriented regex hacks. Malformed YAML, multi-line values, or quoted strings with colons cause subtle bugs.
2. **Agent invocation is hardcoded.** The agent routing logic, model flags, CLI commands, and timeout handling are duplicated across three `case` blocks in `run_agent()`. Adding a new agent or changing the CLI invocation requires editing multiple functions.
3. **Configuration is scattered.** Routes are partially in script variables, partially in environment variables, and partially in the story files themselves (`agent_hint`). There is no single config file a user can edit to change which model handles QA or which personality the Dev agent uses.
4. **No personality injection.** The bash scripts inject headless workflow instructions (`@scripts/headless-skills/dev-story.md`) but do NOT inject the BMAD-method agent personalities (the SKILL.md files in `.agent/skills/`). The personality is implicit in which agent CLI is called (`claude`, `codex`, `gemini`) rather than being an explicit, swappable instruction set.
5. **No structured output from sub-agents.** The bash scripts rely on parsing the story file's YAML frontmatter after the agent exits to determine what happened. They trust the agent to update the file correctly. There is no machine-readable result contract between orchestrator and agent.
6. **Difficult to operationalize.** CI/CD integration, programmatic control, and cross-platform support suffer from bash's inherent limitations.

---

## 2. Solution Overview

A **single Rust binary** (`bmadder`) that replaces the bash orchestrator and supporting Python scripts. It:

- Reads all configuration from a **`bmadder.toml`** file at the project root. Nothing is hardcoded in the binary beyond compiled-in fallback defaults.
- Uses **`pi.dev`** as the universal sub-agent CLI. Instead of shelling out to `claude`, `codex`, or `gemini` directly, the orchestrator invokes `pi.dev` with explicit `--personality`, `--instructions`, `--model`, and `--task` flags.
- Injects **BMAD-method personalities** (SKILL.md files from `.agent/skills/`) as the `--personality` argument, and **headless workflow instructions** (from `scripts/headless-skills/`) as the `--instructions` argument.
- Parses YAML frontmatter with `serde_yaml` for reliable story state management.
- Preserves the exact same project structure, story file format, environment variables, and CLI flags as the bash scripts.
- Is a **drop-in replacement**: users can swap `./scripts/bmadder.sh cycle` for `bmadder cycle` with zero project migration.

### Core Design Principles

1. **Config-driven, not code-driven.** Skills, personalities, model mappings, paths — all in `bmadder.toml`. Editing any of them never requires a recompile. This addresses the core requirement: "the actual skills should stay editable and updateable without having to recompile the rust binary."

2. **Bash is the enforcer, now Rust.** The LLM does work within guardrails. The Rust binary reads frontmatter on disk, validates state transitions, and decides what to invoke next. If an agent claims it's done but didn't update the story file, Rust catches it.

3. **Fresh context per invocation.** Every `pi.dev` call starts clean. No conversation history. Agents discover prior work by reading `progress.txt`, `git log`, story frontmatter, and Implementation Notes. This prevents hallucination drift.

4. **Filesystem is memory.** `progress.txt` is the append-only dev log. `activity.log` is the structured audit trail. Story frontmatter is the state machine. Git history is the source of truth. No database, no service, no hidden state.

5. **Sequential, not parallel.** One story at a time, in dependency order. Parallel execution sounds nice but creates merge conflicts and cross-story contamination. Sequential is boring and correct.

6. **TDD is mandatory.** The Dev agent writes failing tests first, implements until they pass, and runs build/test/lint feedback loops before marking work done. This is enforced by the headless skill instructions, not by the orchestrator.

7. **Protect the codebase at all cost.** Git commits at critical points ensure the codebase is never in an unrecoverable state. If an agent leaves orphaned half-written code, the orchestrator rolls back to the last clean commit.

---

## 3. Config-Driven Architecture (Zero Recompile)

### 3.1 The `bmadder.toml` File

Placed at project root. Created by `bmadder bootstrap`. All paths are relative to the directory containing `bmadder.toml`.

```toml
# bmadder.toml — BMADder orchestrator configuration
# Generated by `bmadder bootstrap`. Edit freely; no recompile needed.

[paths]
skills_dir       = ".agent/skills"             # BMAD agent personality SKILL.md files
headless_dir     = "scripts/headless-skills"   # Consolidated headless workflow instructions
stories_dir      = "docs/backlog/stories"      # Story YAML frontmatter + markdown files
state_dir        = "_bmad"                     # Internal state: progress.txt, activity.log, prompt tmp
prd_file         = "docs/prd.md"               # Product requirements document
architecture_file = "docs/architecture.md"      # System architecture document
orchestrator_marker = "_bmad/orchestrator-master.md"  # Existence = "this project is bootstrapped"

# ---------------------------------------------------------------------------
# MODEL REGISTRY — maps logical model names to pi.dev --model strings
# Add, remove, or rename entries freely. Phase defaults reference these keys.
# ---------------------------------------------------------------------------

[models]
sonnet      = "claude-sonnet-4"
opus        = "claude-opus-4"
gpt5        = "gpt-5"
gemini_pro  = "gemini-2.5-pro"
codex_def   = "codex-default"

# ---------------------------------------------------------------------------
# ROLE → PERSONALITY + MODEL + HEADLESS SKILL MAPPING
#
# Each role maps to:
#   personality  — subdirectory under [paths].skills_dir containing SKILL.md
#   model        — key from [models] above (resolved to pi.dev --model string)
#   headless     — filename under [paths].headless_dir (the workflow instructions)
#
# These drive ALL agent invocations. To swap QA from Claude to GPT-5,
# just change [roles.qa].model from "sonnet" to "gpt5".
# ---------------------------------------------------------------------------

[roles.sm]
personality = "bmad-agent-dev"        # Amelia — acting as Scrum Master
model       = "sonnet"
headless    = "sm-create-stories.md"  # batch sharding (plan / cycle)

[roles.sm_single]
personality = "bmad-agent-dev"        # Amelia — acting as Scrum Master
model       = "sonnet"
headless    = "sm-create-story.md"    # single-story creation (iterative)

[roles.po]
personality = "bmad-agent-pm"         # John — Product Owner
model       = "sonnet"
headless    = "po-review.md"

[roles.dev]
personality = "bmad-agent-dev"        # Amelia — Developer
model       = "gpt5"                  # default; overridden by story agent_hint
headless    = "dev-story.md"

[roles.qa]
personality = "bmad-agent-dev"        # Amelia — QA Auditor
model       = "sonnet"                # sonnet per design; use "opus" for deeper review
headless    = "qa-review.md"

# ---------------------------------------------------------------------------
# AGENT HINT → MODEL OVERRIDE
# When a story's YAML frontmatter has `agent_hint: "codex"`, the orchestrator
# overrides [roles.dev].model with the value mapped here.
# If no mapping exists, [roles.dev].model is used unchanged.
# ---------------------------------------------------------------------------

[agent_hints]
codex  = "gpt5"
claude = "sonnet"
gemini = "gemini_pro"

# ---------------------------------------------------------------------------
# DEFAULTS & LIMITS
# ---------------------------------------------------------------------------

[defaults]
max_dev_iterations       = 10     # Max dev loop iterations per story
max_sm_iterations        = 5      # Max SM↔PO revision loops per story
max_qa_passes            = 3      # Max Dev→QA full-pipeline passes per cycle
story_timeout_seconds    = 1800   # 30 min per pi.dev invocation
gemini_cooldown_seconds  = 15     # Cooldown between Gemini calls
gemini_initial_backoff   = 30     # Starting backoff for 429 rate limits (doubles, caps at 300)

# ---------------------------------------------------------------------------
# PI.DEV INVOCATION TEMPLATE
#
# Variables substituted at runtime:
#   {personality} → full path to the SKILL.md file
#   {headless}    → full path to the headless workflow .md file
#   {prompt_file} → path to _bmad/.prompt-tmp.md (the constructed task prompt)
#   {workspace}   → project root directory
#   {timeout}     → timeout in seconds
#   {model}       → resolved pi.dev --model value from [models]
#
# Note: {prompt_file} is referenced via @ syntax so pi.dev loads it as a file
# rather than as a shell-escaped string (avoids ARG_MAX / quoting issues).
# ---------------------------------------------------------------------------

[pi_dev]
command = "pi.dev"
args    = [
    "--model",        "{model}",
    "--personality",  "{personality}",
    "--instructions", "{headless}",
    "--task",         "@{prompt_file}",
    "--workspace",    "{workspace}",
    "--timeout",      "{timeout}",
    "--json-output"
]
```

### 3.2 How Config Flows at Runtime

```
Startup
  │
  ├─ Find bmadder.toml (walk up from CWD, or --config flag)
  ├─ Deserialize into Config struct (serde)
  ├─ Resolve ALL paths relative to bmadder.toml parent directory
  ├─ Overlay environment variables (see Section 15)
  ├─ Overlay CLI flags (--agent, --timeout, --max-iter, etc.)
  │
  └─ Config is immutable for the rest of the process lifetime
     │
     ├─ Phase functions receive &Config
     ├─ Agent invocations read [roles.*], [agent_hints], [models], [pi_dev]
     ├─ File operations read [paths]
     └─ Logging writes to [paths].state_dir
```

### 3.3 Config Merge Priority

```
CLI flag  >  environment variable  >  bmadder.toml  >  compiled-in default
```

No path in the Rust binary is hardcoded beyond a single bootstrap default for the config file name (`bmadder.toml`). Every model name, personality name, headless filename, and directory path comes from the TOML file at startup.

### 3.4 Why This Design Satisfies the "No Recompile" Requirement

| Change desired | Action required |
|---|---|
| Swap QA model from sonnet to opus | Edit `[roles.qa].model = "opus"` in `bmadder.toml` |
| Use a custom Dev personality | Drop `SKILL.md` into `.agent/skills/bmad-agent-custom/`, set `[roles.dev].personality = "bmad-agent-custom"` |
| Add a new model | Add `llama3 = "llama-3-70b"` to `[models]`, reference it in any role |
| Change the PO review workflow | Edit `scripts/headless-skills/po-review.md` or point `[roles.po].headless` to a different file |
| Use a completely different agent CLI | Edit `[pi_dev].command` and `[pi_dev].args` |
| CI override everything to a fast model | `BMADDER_AGENT=sonnet bmadder cycle` |
| Share config across projects | Copy `bmadder.toml` (it has no secrets, only paths and model names) |

---

## 4. Project Structure Contract

The Rust binary assumes the following on-disk layout (produced by `bmadder bootstrap`):

```
<project-root>/                       # Where bmadder.toml lives
│
├── bmadder.toml                      # Orchestrator config (Section 3)
│
├── docs/
│   ├── prd.md                        # Product Requirements Document (input)
│   ├── architecture.md               # System Architecture Document (input)
│   │
│   ├── backlog/
│   │   └── stories/
│   │       ├── story-0001-auth.md     # Story files (YAML frontmatter + markdown)
│   │       ├── story-0002-db-schema.md
│   │       └── ...
│   │
│   └── standards/                    # Generated by bootstrap (create_rules logic)
│
├── _bmad/                            # Internal state directory
│   ├── orchestrator-master.md        # Existence = "project is bootstrapped"
│   ├── progress.txt                  # Append-only dev log
│   ├── logs/
│   │   └── activity.log              # Structured audit trail
│   ├── .prompt-tmp.md                # Temp file for prompt assembly
│   └── .sm_next_result               # Scratch file for SM next-story path
│
├── _bmad/_config/                    # BMAD module metadata (read-only)
│   ├── manifest.yaml
│   ├── agent-manifest.csv
│   ├── files-manifest.csv
│   └── skill-manifest.csv
│
├── _bmad/bmm/                        # BMAD method module config
│   ├── config.yaml
│   └── ...
│
├── .agent/
│   └── skills/                       # BMAD agent personality SKILL.md files
│       ├── bmad-agent-dev/
│       │   └── SKILL.md              # Amelia — Developer
│       ├── bmad-agent-pm/
│       │   └── SKILL.md              # John — Product Manager
│       ├── bmad-agent-architect/
│       │   └── SKILL.md              # Winston — Architect
│       ├── bmad-agent-analyst/
│       │   └── SKILL.md              # Mary — Business Analyst
│       ├── bmad-agent-tech-writer/
│       │   └── SKILL.md              # Paige — Technical Writer
│       ├── bmad-agent-ux-designer/
│       │   └── SKILL.md              # Sally — UX Designer
│       ├── bmad-create-story/        # Workflow skill (not a persona)
│       │   └── SKILL.md
│       ├── bmad-code-review/         # Workflow skill (not a persona)
│       │   └── SKILL.md
│       └── ...                       # 50+ additional skills
│
└── scripts/
    └── headless-skills/              # Consolidated headless workflow files
        ├── sm-create-stories.md      # SM batch: shard PRD into all stories
        ├── sm-create-story.md        # SM single: create one story
        ├── po-review.md              # PO: story quality review gate
        ├── dev-story.md              # Dev: TDD implementation workflow
        ├── qa-review.md              # QA: code review & acceptance verification
        └── manifest.json             # Headless skill hash manifest (for staleness check)
```

The binary discovers the project root by walking up from CWD looking for a file at the path specified by `[paths].orchestrator_marker` (default: `_bmad/orchestrator-master.md`).

---

## 5. Story File Format & State Machine

### 5.1 Story File Format

Each story file is a markdown file with YAML frontmatter:

```markdown
---
story_id: "STORY-0001"
epic_id: "EPIC-0001"
title: "User Authentication Flow"
status: "READY_FOR_DEV"
priority: "P0"
agent_hint: "codex"
assigned_dev: ""
po_alignment: "APPROVED"
qa_status: ""
created_at: "2026-06-18"
updated_at: "2026-06-18"
links:
  - "docs/architecture.md#authentication"
---

## Context
Brief background on why this story exists and how it fits into the product.

## Requirements
- Functional requirement 1
- Functional requirement 2

## Acceptance Criteria
1. Given X, when Y, then Z
2. ...

## Implementation Notes
(Dev fills this in: files changed, approach, key decisions)

## PO Alignment
(Product Owner review notes and approval/revision decisions)

## QA Notes
(QA Auditor review notes, test results, residual risks)
```

### 5.2 Frontmatter Fields Reference

| Field | Type | Set By | Description |
|---|---|---|---|
| `story_id` | string | SM | Unique identifier, e.g., `STORY-0001` |
| `epic_id` | string | SM | Parent epic, e.g., `EPIC-0001` |
| `title` | string | SM | Concise story title |
| `status` | enum | SM/PO/Dev/QA/Orch | Current state in the pipeline |
| `priority` | string | SM | P0, P1, P2, etc. |
| `agent_hint` | string | SM | Preferred dev agent: `codex`, `claude`, `gemini` |
| `assigned_dev` | string | Orch | Set by orchestrator during dev phase |
| `po_alignment` | enum | PO | `PENDING`, `APPROVED`, `REVISE` |
| `qa_status` | enum | QA | `PASS`, `FAIL`, or empty |
| `created_at` | date | SM | Story creation date |
| `updated_at` | date | * | Last modification date |
| `links` | list | SM | Reference links to architecture, PRD sections, etc. |

### 5.3 Story Status State Machine

```
                    SM creates
                        │
                        ▼
                     DRAFT ──────────────────────┐
                        │                        │
                        │ PO review              │ PO rejects
                        ▼                        │
                  READY_FOR_DEV                  │
                        │                        │
                        │ Orchestrator assigns   │
                        ▼                        │
                      IN_DEV                     │
                        │                        │
                        │ Dev finishes           │
                        ▼                        │
                    PENDING_QA                   │
                        │                        │
                   ┌────┴────┐                   │
                   │         │                   │
              QA PASS    QA FAIL                 │
                   │         │                   │
                   ▼         ▼                   │
              COMPLETED    REFIX ────────────────┘
                           (back to dev queue)

              REVISE: intermediate state set by PO when rejecting.
                      SM must address and set back to DRAFT.
```

Valid status values (enforced by the orchestrator):
- `DRAFT` — SM created or revised, awaiting PO review
- `REVISE` — PO rejected, SM must fix
- `READY_FOR_DEV` — PO approved, queued for implementation
- `IN_DEV` — Developer is actively working on it
- `PENDING_QA` — Dev done, awaiting QA review
- `REFIX` — QA found issues, back to dev queue
- `COMPLETED` — QA passed, story is done

### 5.4 Rust Parsing

The `Story` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryFrontmatter {
    pub story_id: String,
    pub epic_id: Option<String>,
    pub title: String,
    pub status: StoryStatus,
    pub priority: Option<String>,
    pub agent_hint: Option<String>,
    pub assigned_dev: Option<String>,
    pub po_alignment: Option<String>,
    pub qa_status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub links: Vec<String>,
}

pub struct Story {
    pub path: PathBuf,
    pub frontmatter: StoryFrontmatter,
    pub body: String,  // Everything after the --- closing fence
}
```

Parsing:
1. Read the file into a string.
2. Detect the `---` frontmatter fences via regex or line scanning.
3. Parse the YAML block with `serde_yaml`.
4. Everything after the closing `---` is the body.

Writing:
1. Serialize `StoryFrontmatter` to YAML.
2. Prepend `---\n` + YAML + `---\n`.
3. Append the existing body (preserved from read).
4. Write the full string back to the file.

---

## 6. Agent Routing & Sub-Agent Invocation

### 6.1 Routing Priority

```
1. --agent CLI flag (force ALL phases to this agent)
2. BMADDER_AGENT environment variable
3. Story-level agent_hint frontmatter field (dev phase only)
4. Phase default from bmadder.toml [roles.*].model
```

Implementation pseudocode:

```rust
fn resolve_model(config: &Config, phase: &Phase, story: Option<&Story>) -> String {
    // 1. CLI flag overrides everything
    if let Some(agent) = &config.cli_override.agent {
        return config.model_key_for_agent(agent);
    }
    // 2. ENV override
    if let Ok(agent) = std::env::var("BMADDER_AGENT") {
        return config.model_key_for_agent(&agent);
    }
    // 3. Per-phase env override
    let phase_env = match phase {
        Phase::Plan => "BMADDER_PLAN_AGENT",
        Phase::Dev => "BMADDER_DEV_AGENT",
        Phase::QA => "BMADDER_QA_AGENT",
    };
    if let Ok(agent) = std::env::var(phase_env) {
        return config.model_key_for_agent(&agent);
    }
    // 4. Story agent_hint (dev phase only)
    if phase == Phase::Dev {
        if let Some(story) = story {
            if let Some(hint) = &story.frontmatter.agent_hint {
                if let Some(model_key) = config.agent_hints.get(hint) {
                    return config.models.get(model_key).cloned()
                        .unwrap_or_else(|| config.role_default_model(phase));
                }
            }
        }
    }
    // 5. Phase default from config
    config.role_default_model(phase)
}
```

### 6.2 `pi.dev` Sub-Process Invocation

The orchestrator constructs a `pi.dev` command line from the `[pi_dev]` config template:

1. **Look up the role** from `bmadder.toml` → get `personality`, `model` key, `headless` filename.
2. **Resolve full paths:**
   - `personality_path` = `{skills_dir}/{personality}/SKILL.md`
   - `headless_path` = `{headless_dir}/{headless}`
   - `prompt_file` = `{state_dir}/.prompt-tmp.md`
3. **Build the prompt text** and write it to `prompt_file`. The prompt follows the exact HEREDOC pattern from the bash scripts (see Section 7 for per-phase prompt templates).
4. **Substitute variables** into the `[pi_dev].args` template.
5. **Spawn** the subprocess with `std::process::Command`.
6. **Wait** with timeout enforcement.
7. **Parse output:** If `--json-output` produces structured JSON, parse it. Otherwise, check exit code and fall back to reading the story file from disk.

```rust
fn invoke_agent(config: &Config, role: &RoleConfig, prompt: &str,
                story: Option<&Story>, phase: Phase) -> Result<AgentResult> {
    // Build paths
    let personality = config.skills_dir.join(&role.personality).join("SKILL.md");
    let headless = config.headless_dir.join(&role.headless);
    let prompt_file = config.state_dir.join(".prompt-tmp.md");

    // Write prompt
    std::fs::write(&prompt_file, prompt)?;

    // Resolve model
    let model = config.models.get(&role.model)
        .cloned()
        .unwrap_or_else(|| role.model.clone());

    // Build command from template
    let mut cmd = Command::new(&config.pi_dev.command);
    for arg in &config.pi_dev.args {
        let resolved = arg
            .replace("{personality}", &personality.to_string_lossy())
            .replace("{headless}", &headless.to_string_lossy())
            .replace("{prompt_file}", &prompt_file.to_string_lossy())
            .replace("{workspace}", &config.project_root.to_string_lossy())
            .replace("{timeout}", &config.defaults.story_timeout_seconds.to_string())
            .replace("{model}", &model);
        cmd.arg(resolved);
    }

    // Execute with timeout
    let output = cmd
        .current_dir(&config.project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;  // simplified; real impl uses timeout

    // Parse result
    if let Ok(json) = serde_json::from_slice::<PiDevOutput>(&output.stdout) {
        return Ok(AgentResult::from_pi_dev(json));
    }
    // Fallback: check exit code
    Ok(AgentResult {
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}
```

### 6.3 Timeout & Rate-Limit Handling

- **Timeout:** The orchestrator enforces `[defaults].story_timeout_seconds` by wrapping the subprocess in a timeout. If the agent exceeds the timeout, the process is killed. The orchestrator logs the timeout and treats it as a failure.
- **Gemini rate limits:** When using the `gemini_pro` model, the orchestrator scans stdout/stderr for rate-limit signatures (`429`, `rateLimitExceeded`, `MODEL_CAPACITY_EXHAUSTED`, `No capacity available`). If detected, it applies exponential backoff (starting at `gemini_initial_backoff` seconds, doubling each time, capping at 300 seconds) and returns a "retry" signal to the caller.
- **Gemini cooldown:** A fixed `gemini_cooldown_seconds` delay (default 15s) is inserted between any two Gemini calls to stay under quota.
- **Per-story reset:** Gemini backoff state resets at the start of each new story.

### 6.4 Prompt Template Pattern

Every prompt follows the same structure established by the bash scripts:

```
You are the <ROLE> running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated headless skill instructions:
@scripts/headless-skills/<headless-file>

Working on ONE story:
  ID: <story_id>
  File: @<story_file>

Context documents:
@docs/prd.md
@docs/architecture.md
@_bmad/progress.txt

<role-specific instructions>
<completion criteria>
<rules and constraints>
```

The `@` references are file paths that `pi.dev` resolves relative to the workspace root. This avoids inline prompt bloat and keeps the task description concise.

---

## 7. Phase Implementations

### 7.1 `bmadder plan` — SM + PO Batch Planning

**Preconditions:**
- `docs/prd.md` and `docs/architecture.md` exist and have substantive content.
- Project is bootstrapped (`_bmad/orchestrator-master.md` exists).

**Flow:**

```
plan
│
├─ Validate preconditions
├─ Run auth preflight (Section 13)
│
├─ Step 1: Scrum Master (batch story creation)
│   │
│   ├─ Resolve agent: config.roles.sm (personality + model + headless)
│   ├─ Build prompt using sm-create-stories.md headless skill
│   ├─ Prompt instructs SM to:
│   │   - Read PRD + architecture
│   │   - List existing stories in docs/backlog/stories/
│   │   - Create NEW story files for unimplemented features
│   │   - Skip stories already at READY_FOR_DEV or COMPLETED
│   │   - For REVISE stories: address PO notes, update content, set back to DRAFT
│   │   - Set frontmatter: status: "DRAFT", po_alignment: "PENDING"
│   │   - Log summary to _bmad/logs/activity.log
│   │   - Do NOT implement code, do NOT approve stories
│   ├─ Invoke pi.dev
│   ├─ Validate story frontmatter (ensure valid statuses)
│   └─ Log: count of DRAFT stories created
│
├─ Step 2: Product Owner (batch review) — unless --skip-po
│   │
│   ├─ Resolve agent: config.roles.po (personality + model + headless)
│   ├─ Build prompt using po-review.md headless skill
│   ├─ Prompt instructs PO to:
│   │   - Read EVERY story with status: "DRAFT"
│   │   - Evaluate each against checklist criteria:
│   │     1. Maps to at least one PRD requirement
│   │     2. Consistent with architecture (layers, patterns, naming)
│   │     3. Requirements are clear, specific, unambiguous
│   │     4. Acceptance Criteria are numbered, testable, specific
│   │     5. Scope is right-sized for one implementation effort
│   │     6. Dependencies are explicit
│   │     7. agent_hint is set correctly
│   │     8. No duplicate scope with other stories
│   │   - If ALL criteria pass: status → "READY_FOR_DEV", po_alignment → "APPROVED"
│   │   - If ANY fails: status → "REVISE", po_alignment → "REVISE" with specific notes
│   │   - Log decisions to activity.log
│   ├─ Invoke pi.dev
│   └─ Log: count READY_FOR_DEV vs REVISE
│
├─ If --skip-po: auto-approve all DRAFT → READY_FOR_DEV, po_alignment → "APPROVED"
│
└─ Log final result to progress.txt
```

### 7.2 `bmadder dev` — Sequential Development Loop

**Queue assembly:**
1. READY_FOR_DEV stories (sorted by filename, which encodes dependency order).
2. REFIX stories (appended after READY_FOR_DEV).

**Per-story loop:**

```
dev
│
├─ Commit uncommitted worktree changes (pre-dev snapshot)
│   git add -A && git commit -m "chore: pre-dev worktree snapshot"
│
├─ for each story in queue:
│   │
│   ├─ Resolve agent for THIS story:
│   │   - Check agent_hint in story frontmatter
│   │   - Override config.roles.dev.model if hint maps to a known model
│   │   - personality + headless always from config.roles.dev
│   │
│   ├─ Reset Gemini backoff state
│   ├─ Update story status → "IN_DEV"
│   │
│   ├─ for iteration in 1..=max_dev_iterations:
│   │   │
│   │   ├─ Build prompt using dev-story.md headless skill
│   │   ├─ Prompt instructs Dev to:
│   │   │   - Read story file (Requirements, Acceptance Criteria)
│   │   │   - Read architecture.md, prd.md, progress.txt
│   │   │   - Run: git log --oneline -20
│   │   │   - Implement the story (TDD: tests first, then code)
│   │   │   - Run build/test/lint feedback loops
│   │   │   - When ALL ACs met AND build/test/lint pass:
│   │   │     → status: "PENDING_QA"
│   │   │     → Fill in ## Implementation Notes
│   │   │     → Append to progress.txt
│   │   │     → git add -A && git commit -m "feat(STORY-NNNN): <summary>"
│   │   │   - If cannot finish: commit partial progress, leave IN_DEV
│   │   │   - ONLY work on this story, do NOT touch other stories
│   │   │   - Do NOT skip feedback loops
│   │   ├─ Invoke pi.dev
│   │   │
│   │   ├─ Read story status from disk (NEVER trust agent output)
│   │   ├─ If status == "PENDING_QA" → break (success)
│   │   ├─ If status == "COMPLETED" → break (already done)
│   │   ├─ If Gemini and more iterations remain → 20s cooldown
│   │   └─ Continue looping
│   │
│   ├─ If max iterations reached and status ≠ PENDING_QA/COMPLETED:
│   │   └─ Log STALLED
│   │
│   └─ next story
│
└─ Log summary to progress.txt
```

### 7.3 `bmadder qa` — Sequential QA Audit

**Queue:** All PENDING_QA stories.

```
qa
│
├─ for each story in queue:
│   │
│   ├─ Resolve agent: config.roles.qa (personality + model + headless)
│   │
│   ├─ Build prompt using qa-review.md headless skill
│   ├─ Prompt instructs QA to:
│   │   - Read story Requirements, Acceptance Criteria, Implementation Notes
│   │   - Review code files listed in Implementation Notes
│   │   - Run the test suite
│   │   - Verify EACH acceptance criterion against implementation
│   │   - Check for regressions vs PRD and architecture
│   │   - If ALL checks pass:
│   │     → qa_status: "PASS", status: "COMPLETED"
│   │     → Append ## QA Notes: what tested, how, residual risks
│   │     → Do NOT git commit (orchestrator handles)
│   │   - If ANY check fails:
│   │     → qa_status: "FAIL", status: "REFIX"
│   │     → Append ## QA Notes: what failed, steps to reproduce, fix guidance
│   │   - Log decisions to activity.log
│   ├─ Invoke pi.dev
│   │
│   ├─ Enforce outcomes (NEVER trust agent output alone):
│   │   ├─ If status == "COMPLETED":
│   │   │   ├─ git add -A && git commit -m "story(STORY-NNNN): <title> [QA PASS]"
│   │   │   ├─ git push (best-effort, warn on failure)
│   │   │   └─ Log QA PASS
│   │   ├─ If status == "REFIX":
│   │   │   └─ Log QA FAIL
│   │   └─ If status is ANYTHING ELSE:
│   │       ├─ Force: status → "REFIX", qa_status → "FAIL"
│   │       └─ Log QA FORCED REFIX (ambiguous result)
│   │
│   └─ next story
│
└─ Log summary
```

### 7.4 `bmadder cycle` — Full Batch Pipeline

```
cycle
│
├─ If no READY_FOR_DEV or REFIX stories exist:
│   ├─ If DRAFT stories exist and no REVISE: skip SM, run PO only
│   ├─ If REVISE stories exist: run SM (to address revisions) then PO
│   └─ run_plan()
│
├─ for pass in 1..=max_qa_passes:
│   ├─ run_dev()
│   ├─ run_qa()
│   ├─ Count REFIX stories
│   └─ If REFIX == 0 → break
│
├─ Show final status (Section 11.3)
├─ If ALL stories COMPLETED: print "ALL N STORIES COMPLETED"
└─ Otherwise: print completed/total, show stalled/REFIX/IN_DEV counts
```

### 7.5 `bmadder iterative` — Story-by-Story Pipeline

This is the more sophisticated pipeline that processes one story through its ENTIRE lifecycle (SM→PO→Dev→QA→commit) before moving to the next.

```
iterative
│
├─ Validate: prd.md + architecture.md exist
├─ Run auth preflight
├─ Commit uncommitted worktree changes (pre-pipeline snapshot)
│
├─ Step 1: Resume in-flight stories
│   │
│   ├─ If --from-existing:
│   │   └─ Queue: READY_FOR_DEV + REFIX (skip SM/PO phase)
│   ├─ Else:
│   │   └─ Queue: DRAFT + REVISE + READY_FOR_DEV + REFIX + IN_DEV + PENDING_QA
│   │
│   └─ for each in-flight story:
│       └─ process_one_story(story_file)
│
├─ Step 2: SM-driven loop (create new stories from PRD)
│   │
│   └─ while iterations < max_stories (safety limit: 100):
│       │
│       ├─ SM creates NEXT story
│       │   ├─ Resolve agent: config.roles.sm_single
│       │   ├─ Build prompt using sm-create-story.md headless skill
│       │   ├─ Prompt instructs SM to:
│       │   │   ├─ Read PRD + architecture + progress.txt
│       │   │   ├─ Run: git log --oneline -30
│       │   │   ├─ Review existing stories
│       │   │   ├─ IF PRD has unimplemented features:
│       │   │   │   → Create ONE story file (story-NNNN-slug.md)
│       │   │   │   → Respect dependencies (foundational first)
│       │   │   │   → Set: status: "DRAFT", po_alignment: "PENDING"
│       │   │   │   → Log: "SM_NEXT: created STORY-NNNN -- <title>"
│       │   │   └─ IF PRD fully implemented:
│       │   │       → Write "ALL_DONE: PRD fully implemented." to progress.txt
│       │   ├─ Invoke pi.dev
│       │   ├─ Detect new story file (diff directory before/after)
│       │   └─ If ALL_DONE in progress.txt → break (pipeline complete)
│       │
│       └─ If new story created:
│           └─ process_one_story(new_story)
│               │
│               ├─ Phase 1: SM↔PO Approval Loop
│               │   │
│               │   ├─ for iter in 1..=max_sm_iterations:
│               │   │   ├─ sm_write_story() — SM creates/revises story content
│               │   │   │   ├─ If status == "DRAFT" and content mostly empty:
│               │   │   │   │   → Write full story following workflow + checklist
│               │   │   │   ├─ If status == "REVISE":
│               │   │   │   │   → Read ## PO Alignment for revision notes
│               │   │   │   │   → Address EVERY issue
│               │   │   │   │   → Set: status: "DRAFT", po_alignment: "PENDING"
│               │   │   │   │   → Append dated revision summary
│               │   │   │   └─ Do NOT implement code, do NOT approve
│               │   │   │
│               │   │   ├─ Skip PO if --skip-po → auto-approve, break
│               │   │   │
│               │   │   └─ po_review_story()
│               │   │       ├─ Evaluate against checklist (same as plan PO, but one story)
│               │   │       ├─ If ALL pass: status → "READY_FOR_DEV", po_alignment → "APPROVED", return 0
│               │   │       └─ If ANY fails: status → "REVISE", po_alignment → "REVISE", return 1
│               │   │
│               │   └─ If stalled after max iterations: log STALLED, skip story
│               │
│               ├─ Phase 2: Dev↔QA Implementation Loop
│               │   │
│               │   ├─ for cycle in 1..=max_dev_iterations:
│               │   │   │
│               │   │   ├─ Dev sub-loop (internal until PENDING_QA):
│               │   │   │   ├─ Resolve agent for this story (checks agent_hint)
│               │   │   │   ├─ Set status → "IN_DEV"
│               │   │   │   ├─ for dev_iter in 1..=max_dev_iterations:
│               │   │   │   │   ├─ dev_implement_story() — same as 7.2 per-story loop
│               │   │   │   │   ├─ If status == "PENDING_QA" → break
│               │   │   │   │   └─ Continue
│               │   │   │   └─ If stalled: log STALLED, return error
│               │   │   │
│               │   │   └─ QA review:
│               │   │       ├─ qa_review_story() — same as 7.3 per-story review
│               │   │       ├─ If QA PASS → break (story complete)
│               │   │       └─ If QA FAIL → story is REFIX, loop back to Dev
│               │   │
│               │   └─ If stalled: log STALLED
│               │
│               └─ If Dev↔QA passed:
│                   ├─ commit_story() — git commit + push
│                   └─ Log COMPLETE
│
└─ Final report:
    ├─ Completed this run: N
    ├─ Stalled: M
    ├─ Total COMPLETED stories on disk: T
    └─ If ALL_DONE in progress.txt: "PRD FULLY IMPLEMENTED"
```

### 7.6 `bmadder status` — Status Display

Displays:
- Story counts by status (color-coded):
  - COMPLETED → green
  - READY_FOR_DEV → blue
  - IN_DEV → blue
  - DRAFT / REVISE / PENDING_QA → yellow
  - REFIX → red
- Total story count
- Key file existence checks: `docs/prd.md`, `docs/architecture.md`, `_bmad/orchestrator-master.md`, `_bmad/progress.txt`
- Current agent routing configuration (plan → model, dev → model, qa → model)
- Global override warning if `--agent` or `BMADDER_AGENT` is active

### 7.7 `bmadder validate` — Story Validation

- Parse all story files
- Validate frontmatter: required fields present, valid status values
- Report any invalid statuses, missing required fields, or structural issues

---

## 8. Pipeline Modes: Batch vs Iterative

### 8.1 Comparison

| Aspect | `bmadder cycle` (batch) | `bmadder iterative` |
|---|---|---|
| **Story creation** | SM shards entire PRD into all stories in one pass | SM creates one story at a time, sequentially |
| **PO review** | PO reviews all DRAFT stories at once | PO reviews one story per SM pass (SM↔PO loop) |
| **Dev + QA** | Dev processes all READY_FOR_DEV, then QA all PENDING_QA | Full SM→PO→Dev→QA→commit cycle per story |
| **Deployable after** | All stories pass all phases | Each individual story passes QA |
| **Git commits** | After each QA pass | After PO approval + after each QA pass |
| **Crash recovery** | Re-runs all incomplete work | Picks up at exact story, cleans orphaned code |
| **Best for** | Well-defined PRDs where full scope is known upfront | Projects where each increment should be testable/deployable immediately |

### 8.2 Recommended Combined Workflow

```bash
# Step 1: SM shards PRD into all story stubs, PO reviews and approves (one-time batch)
bmadder plan

# Step 2: Implement stories one at a time — each story is a deployable commit
bmadder iterative --from-existing
```

This gives full backlog visibility (all stories planned upfront) with incremental deployability (each story committed and testable as it completes).

---

## 9. Git Integration & Codebase Safety

### 9.1 Pre-Dev Snapshot

Before any dev loop begins, the orchestrator commits any uncommitted worktree changes:

```bash
git add -A && git commit -m "chore: pre-dev worktree snapshot"
```

This ensures the sub-agent never sees a dirty working tree and never prompts about it.

### 9.2 PO Approval Commit

After the PO approves stories in the iterative pipeline (SM↔PO loop), the orchestrator commits:

```bash
git add -A && git commit -m "chore: PO approved stories for dev"
```

This creates a rollback point: if a dev agent corrupts the codebase, the orchestrator can reset to this commit.

### 9.3 QA Pass Commit

After QA passes a story:

```bash
git add -A && git commit -m "story($story_id): $title [QA PASS]"
git push  # best-effort, warn on failure
```

### 9.4 Rust Implementation

Use either:
- The `git2` crate for programmatic git operations.
- Shell out to the `git` binary via `std::process::Command` (simpler, no libgit2 linking issues).

All git operations are best-effort: failures are logged as warnings but do not halt the pipeline.

### 9.5 Orphaned Code Protection

On crash/restart during a story that was `IN_DEV`:
- The orchestrator runs `git checkout .` to discard uncommitted changes.
- Resets the story status to `READY_FOR_DEV` (or `REFIX` if coming from QA failure).
- The story gets re-implemented from scratch on next run.
- Rationale: "An incomplete story can be redone — orphaned half-written code and broken imports are far more dangerous."

---

## 10. Crash Recovery & Resume

### 10.1 Stateless Orchestrator

The orchestrator is stateless between invocations. ALL state lives on disk:
- Story frontmatter (`status`, `po_alignment`, `qa_status`)
- `_bmad/progress.txt` (append-only dev log)
- `_bmad/logs/activity.log` (structured audit trail)
- Git history (source of truth for code changes)

This makes crash recovery trivial: just re-run the command. The orchestrator discovers what needs to be done by scanning story statuses on disk.

### 10.2 Resume Strategy

**`bmadder.sh cycle` (batch):**
- Re-run `bmadder cycle`. It detects existing READY_FOR_DEV stories (skips plan), processes remaining dev queue, then QA.

**`bmadder iterative`:**
- Re-run `bmadder iterative`.
- **Step 1:** Discovers in-flight stories (DRAFT, REVISE, READY_FOR_DEV, REFIX, IN_DEV, PENDING_QA) and resumes them.
- **IN_DEV reset:** On detecting an `IN_DEV` story, the orchestrator runs `git checkout .` to discard orphaned code, resets the story to `READY_FOR_DEV` (or `REFIX`), and re-implements from scratch.
- **PENDING_QA preserved:** Stories at PENDING_QA are NOT reset (their code is committed at the dev-complete point).
- **Step 2:** SM continues creating new stories from the PRD.

### 10.3 `--start-from` Flag

Skip all stories before a given story ID. Used when a mid-backlog story stalled and the user wants to resume from there:

```bash
bmadder iterative --start-from STORY-0005
```

---

## 11. Logging, Activity Tracking & Status Display

### 11.1 Activity Log (`_bmad/logs/activity.log`)

TSV-like structured format:

```
2026-06-18T14:30:00Z | ORCH | -       | CYCLE_START   | Full cycle
2026-06-18T14:30:05Z | ORCH | -       | SM_START      | SM sharding via claude-sonnet-4
2026-06-18T14:35:12Z | SM   | -       | SM_DONE       | Sharding complete
2026-06-18T14:35:15Z | ORCH | -       | PO_START      | PO review via claude-sonnet-4
2026-06-18T14:40:00Z | PO   | -       | PO_DONE       | Review complete
2026-06-18T14:40:01Z | ORCH | STORY-0001 | DEV_START  | Dev loop via gpt-5
2026-06-18T14:50:30Z | DEV  | STORY-0001 | DEV_DONE   | 3 iterations via gpt-5
2026-06-18T14:50:31Z | ORCH | STORY-0001 | QA_START   | QA via claude-sonnet-4
2026-06-18T14:55:00Z | QA   | STORY-0001 | QA_PASS    | Completed
2026-06-18T14:55:01Z | GIT  | STORY-0001 | COMMIT     | story(STORY-0001): QA PASS
```

Format: `timestamp | phase_actor | story_id | event | detail`

### 11.2 Progress Log (`_bmad/progress.txt`)

Append-only, human-readable:

```
2026-06-18T14:35:12Z | PLAN: 5 approved, 1 need revision
2026-06-18T14:50:30Z | STORY-0001: DEV done, 3 iters, gpt-5
2026-06-18T14:55:00Z | STORY-0001: QA PASS
```

### 11.3 Console Output

Colorized using ANSI escape codes (via the `colored` crate or `console` crate), matching the bash script's color scheme:

| Level | Color | Bash Equivalent |
|---|---|---|
| `[INFO]` | Blue | `BLUE='\033[0;34m'` |
| `[OK]` | Green | `GREEN='\033[0;32m'` |
| `[WARN]` | Yellow | `YELLOW='\033[1;33m'` |
| `[ERR]` | Red | `RED='\033[0;31m'` |
| Phase banners | Cyan | `CYAN='\033[0;36m'` |
| Iterative-phase | Magenta | `MAGENTA='\033[0;35m'` |

Status display:

```
===================================================
  BMADder Status
===================================================

  COMPLETED      3
  REFIX          1
  READY_FOR_DEV  2
  IN_DEV         1
  DRAFT          0
  REVISE         0
  PENDING_QA     0

  Total: 7

  Key Files:
  [OK] docs/prd.md
  [OK] docs/architecture.md
  [OK] _bmad/orchestrator-master.md
  [OK] _bmad/progress.txt

  Agent Routing:
  plan -> claude-sonnet-4    dev -> gpt-5    qa -> claude-sonnet-4
```

---

## 12. Bootstrap Module

`bmadder bootstrap` replaces `bootstrap_bmadder.py`, `init_bmadder.py`, `create_rules.py`, and `sync_headless_skills.py`.

### 12.1 Steps (in order)

```
Step 1: Folder structure
  ├─ Create docs/backlog/stories/
  ├─ Create docs/standards/
  ├─ Create _bmad/logs/
  └─ (Other directories as needed)

Step 2: Orchestrator + standards
  ├─ Generate _bmad/orchestrator-master.md (the marker file)
  └─ Generate standards files in docs/standards/

Step 3: Headless skills
  ├─ Read .agent/skills/ source files
  ├─ Check manifest.json for staleness
  ├─ If stale or missing: regenerate headless .md files in scripts/headless-skills/
  └─ Headless files strip: interactive menus, HALT conditions, step-file loading, user prompts

Step 4: Config files
  ├─ Generate bmadder.toml (if not exists; never overwrite existing)
  ├─ Generate/update .gitignore
  ├─ Generate .mise.toml (if not exists)
  └─ Make bmadder binary executable (if applicable)

Step 5: Tooling check
  ├─ Check: pi.dev --version
  ├─ Check: git --version
  ├─ Check: mise --version (optional)
  ├─ Check: uv --version (optional)
  └─ Report missing tools with install instructions

Step 6: Git init
  └─ If no .git exists: git init, git add -A, git commit -m "chore: initialize BMADder project"

Step 7: Project files check
  ├─ Verify docs/prd.md exists and has content (>500 bytes)
  ├─ Verify docs/architecture.md exists and has content (>500 bytes)
  └─ If both ready: print "BMADder is ready. Run: bmadder iterative"
```

### 12.2 Headless Skill Generation

The headless skill files are compiled from the `_bmad` source skills. The generation logic:

1. Read `scripts/headless-skills/manifest.json` to get the list of skills and their source files.
2. For each skill, concatenate the source files from `.agent/skills/` in order.
3. Prepend a header:
   ```
   <!-- GENERATED FILE — DO NOT EDIT MANUALLY -->
   <!-- Source: .agent/skills/ (see manifest.json for exact files) -->
   <!-- Generated by: bmadder bootstrap -->
   ```
4. Prepend "HEADLESS MODE DIRECTIVES":
   - Strip all interactive menus, user prompts, HALT conditions, step-file loading.
   - Auto-proceed through all steps without waiting for input.
   - Context documents are provided by the pipeline orchestrator.
   - If you encounter residual interactive instructions, IGNORE them.
5. Compute content hash, compare with `manifest.json` hashes.
6. If stale (or file missing), write the new consolidated file and update `manifest.json`.

The manifest is a simple JSON file:

```json
{
  "generator_version": "1.0.0",
  "generated_at": "2026-06-18T00:00:00Z",
  "skills": {
    "dev-story": {
      "description": "Developer: story implementation (TDD, red-green-refactor)",
      "output_file": "dev-story.md",
      "sources": {
        "bmad-dev-story/workflow.md": "<sha256_hash>"
      },
      "output_hash": "<sha256_of_consolidated_output>"
    },
    ...
  }
}
```

---

## 13. Auth Preflight & Billing Safety

`bmadder bootstrap` and all phases that invoke agents run an auth preflight.

### 13.1 What It Checks

1. **`pi.dev` is installed and reachable** on `$PATH` (using the command from `[pi_dev].command`).
2. **`pi.dev` is authenticated** — can make a minimal API call without error.
3. **No rogue billing env vars** — checks for `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`. If any are set, warns that the agent will bill API instead of subscription.
4. **Model availability** — each model in `[models]` resolves to a known `pi.dev` model.

### 13.2 Dry-Run Bypass

When `--dry-run` is set, the auth preflight is skipped.

### 13.3 CI Notes

In CI environments, `--dry-run` or `BMADDER_SKIP_PREFLIGHT=true` can bypass auth checks.

---

## 14. CLI Surface

### 14.1 Binary Name

`bmadder`

### 14.2 Subcommands

```
bmadder plan              SM shards PRD into all stories, PO reviews all at once
bmadder dev               Sequential dev loop (all READY_FOR_DEV + REFIX)
bmadder qa                Sequential QA audit (all PENDING_QA)
bmadder cycle             Full pipeline: plan → dev → qa (with REFIX loops)
bmadder iterative         Story-by-story: SM→PO→Dev→QA→commit per story
bmadder status            Show story states and key file checks
bmadder validate          Validate story frontmatter only
bmadder bootstrap         One-command project setup
```

### 14.3 Global Flags

| Flag | Default | Description |
|---|---|---|
| `--config <PATH>` | auto-discover | Path to bmadder.toml |
| `--max-iter N` | 10 | Max dev iterations per story (batch) |
| `--max-sm-iter N` | 5 | Max SM↔PO loops per story (iterative) |
| `--max-dev-iter N` | 10 | Max Dev↔QA loops per story (iterative) |
| `--dry-run` | false | Show what would run without executing |
| `--skip-po` | false | Skip PO gate (auto-approve all drafts) |
| `--skip-sm` | false | Skip SM phase (use existing stories) |
| `--agent AGENT` | — | Force ALL phases to use this agent |
| `--no-commit` | false | Skip git commit/push after QA pass |
| `--timeout SECS` | 1800 | Max seconds per agent invocation |
| `--story ID` | — | Target a specific story (e.g., `STORY-0001`) |
| `--from-existing` | false | Skip SM/PO loop; use stories at READY_FOR_DEV/REFIX |
| `--start-from ID` | — | Skip stories before this ID (resume mid-backlog) |
| `--json` | false | Output status/results in JSON (for CI/scripts) |

---

## 15. Environment Variable Overlay

All environment variables from the original bash scripts are preserved. They override the corresponding `bmadder.toml` values at runtime.

| Variable | Overrides | Default |
|---|---|---|
| `BMADDER_AGENT` | All phase agents | — |
| `BMADDER_PLAN_AGENT` | `[roles.sm].model` + `[roles.po].model` | sonnet |
| `BMADDER_DEV_AGENT` | `[roles.dev].model` | gpt5 |
| `BMADDER_QA_AGENT` | `[roles.qa].model` | sonnet |
| `BMADDER_MAX_ITER` | `[defaults].max_dev_iterations` | 10 |
| `BMADDER_MAX_SM_ITER` | `[defaults].max_sm_iterations` | 5 |
| `BMADDER_MAX_DEV_ITER` | `[defaults].max_dev_iterations` | 10 |
| `BMADDER_STORY_TIMEOUT` | `[defaults].story_timeout_seconds` | 1800 |
| `BMADDER_CONFIG` | Path to bmadder.toml | auto-discover |
| `BMADDER_SKIP_PREFLIGHT` | Skip auth preflight | false |

Env vars are resolved at startup during config loading. The merge order:

```
CLI flag  >  env var  >  bmadder.toml value  >  compiled-in default
```

---

## 16. Crate Structure

```
bmadder/
├── Cargo.toml                    # Workspace root
│
├── bmadder-cli/                  # [[bin]] bmadder
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               # clap CLI definition, find bmadder.toml, dispatch to phases
│       ├── config.rs             # Config struct (serde), path resolution, env overlay, defaults
│       ├── bootstrap.rs          # Bootstrap module (folder structure, bmadder.toml, headless skills)
│       ├── story.rs              # Story struct, frontmatter parser (serde_yaml), status enum
│       ├── agent.rs              # pi.dev subprocess: prompt builder, command construction, timeout
│       ├── phases/
│       │   ├── mod.rs
│       │   ├── plan.rs           # Section 7.1
│       │   ├── dev.rs            # Section 7.2
│       │   ├── qa.rs             # Section 7.3
│       │   ├── cycle.rs          # Section 7.4
│       │   ├── iterative.rs      # Section 7.5
│       │   ├── status.rs         # Section 7.6
│       │   └── validate.rs       # Section 7.7
│       ├── logging.rs            # activity.log, progress.txt, colored console output
│       ├── git.rs                 # Git integration (snapshot, commit, push)
│       ├── preflight.rs          # Auth preflight (pi.dev + model availability)
│       └── utils.rs              # File I/O helpers, path resolution, YAML read/write
│
└── bmadder-core/                 # [lib] shared types
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── story.rs              # Story, StoryFrontmatter, StoryStatus (re-exported)
        ├── config.rs             # Config, RoleConfig, ModelMap, PiDevConfig
        └── agent.rs              # AgentResult, PiDevOutput
```

### 16.1 Why Two Crates?

- `bmadder-core`: pure data types, no I/O. Could be used by tooling, CI plugins, or future GUI wrappers without pulling in CLI dependencies.
- `bmadder-cli`: the binary. Depends on `bmadder-core` + all I/O and subprocess crates.

(The split is optional for v1 — a single crate is fine to start.)

---

## 17. Key Dependencies

| Crate | Purpose |
|---|---|
| `clap` (v4, derive) | CLI argument parsing, subcommands, help text |
| `serde` + `serde_derive` | Serialization framework |
| `serde_yaml` | YAML frontmatter parsing/writing |
| `serde_json` | JSON output mode (`--json`), pi.dev stdout parsing |
| `toml` | `bmadder.toml` deserialization |
| `colored` or `console` | Terminal color output |
| `chrono` | Timestamps for logging |
| `regex` | Frontmatter fence detection, rate-limit pattern matching |
| `git2` OR `std::process::Command` | Git operations |
| `tempfile` | Temp files (optional; can use `_bmad/.prompt-tmp.md` directly) |
| `walkdir` | Walk up directory tree to find `bmadder.toml` and `_bmad/orchestrator-master.md` |
| `sha2` | Content hashing for headless skill staleness checks |

---

## 18. Testing & Validation Strategy

### 18.1 Unit Tests (in `bmadder-core`)

- **Frontmatter round-trip:** Parse a complete story file → modify a field → write → re-read → assert equality.
- **Status validation:** Reject invalid statuses. Accept valid transitions.
- **Agent routing priority:** Test CLI flag > env var > agent_hint > default ordering.
- **Story collection:** Filter by status, sort by filename, filter by `--story`, filter by `--start-from`.
- **Config parsing:** Deserialize a complete `bmadder.toml` → assert all fields resolved correctly.
- **Config defaults:** Deserialize minimal TOML → assert compiled-in defaults fill missing fields.
- **Path resolution:** All paths relative to config file parent → absolute paths.

### 18.2 Integration Tests (in `bmadder-cli`)

- **Bootstrap on empty directory:** Verify folder structure created, `bmadder.toml` written, `.gitignore` updated, git repo initialized.
- **Bootstrap on existing project:** Verify existing `bmadder.toml` is NOT overwritten.
- **`bmadder plan --dry-run`** on fixture project: Verify SM prompt text, PO prompt text, correct agent routing.
- **`bmadder iterative --from-existing --dry-run`** on fixture project: Verify stories processed in correct order, agent_hint overrides applied correctly.
- **`bmadder status`:** Verify output format matches expected counts.
- **`bmadder validate`:** Verify invalid status reported, valid status silent.
- **Headless skill generation:** Verify stripped of interactive artifacts, hash matches source.

### 18.3 End-to-End Test

Take a real small project (e.g., "todo CLI app" with a 5-story backlog, PRD, and architecture doc), run `bmadder iterative`, and verify:
- Each story completes (status → COMPLETED).
- Each git commit is present with correct message format.
- The final MVP builds and passes its test suite.
- `bmadder status` shows all stories COMPLETED.
- `progress.txt` and `activity.log` are fully populated.

---

## 19. Migration Path

### 19.1 Drop-In Replacement

The Rust binary reads the exact same:
- Project structure
- Story file format (YAML frontmatter + markdown)
- Story status values and state machine
- Environment variable names
- CLI flag names

A user can replace `./scripts/bmadder.sh cycle` with `bmadder cycle` with zero migration.

### 19.2 Headless Skills Remain Compatible

The headless skill `.md` files in `scripts/headless-skills/` do not change format. The `bmadder bootstrap` command regenerates them the same way `sync_headless_skills.py` does today.

### 19.3 BMAD Agent SKILL.md Files Unchanged

The `.agent/skills/bmad-agent-*/SKILL.md` files are passed directly to `pi.dev` as `--personality` arguments. No format changes needed.

### 19.4 `.agent/skills/` Works Natively

Since `pi.dev` reads SKILL.md files directly (just as Claude Code, Codex, or Gemini CLI do today), the existing skill library works without modification.

### 19.5 Coexistence

The Rust binary and the bash scripts can coexist in the same project. They read the same files. The `bmadder.toml` config is new but has no overlap with any existing file.

---

## 20. What Changes vs What Stays the Same

| Aspect | Bash (current) | Rust (target) |
|---|---|---|
| **Orchestrator language** | Bash (~1,000 lines) + Python (~500 lines) | Rust CLI binary (~2,000-3,000 lines) |
| **Config location** | Hardcoded in script variables + env vars | Single `bmadder.toml` at project root |
| **Agent invocation** | `claude -p`, `codex exec`, `gemini --yolo` directly | Unified: `pi.dev --model X --personality Y --instructions Z --task @prompt` |
| **Personality loading** | Implicit per agent CLI | Explicit `pi.dev --personality .agent/skills/bmad-agent-dev/SKILL.md` |
| **Headless skills** | `@scripts/headless-skills/*.md` via prompt references | Same; passed as `pi.dev --instructions` or `@` references in task |
| **State machine** | Bash reads YAML via `sed` + `grep` | Rust parses YAML properly via `serde_yaml` |
| **Story file format** | YAML frontmatter + markdown | Identical (unchanged) |
| **Project structure** | As defined in bash scripts | Identical (unchanged) |
| **Multi-model routing** | Hardcoded in `agent_model_flags()` | Configurable in `[models]`, `[roles.*]`, and `[agent_hints]` sections of `bmadder.toml` |
| **Adding a new agent** | Edit 3 `case` blocks in `run_agent()` | Add entry to `[models]`, reference in `[roles.*].model` |
| **Changing a personality** | Not supported (implicit) | Drop new SKILL.md, update `[roles.*].personality` in TOML |
| **CI override** | `BMADDER_AGENT=claude` env var | Same env vars, overlaid on top of TOML |
| **Logging** | `echo >> file` | Structured append via Rust file handles |
| **Parallelism** | Sequential (by design) | Sequential (preserved design philosophy) |
| **Crash recovery** | Bash trap + `git checkout` | Rust reads disk state, resets IN_DEV stories, resumes |
| **Output format** | ANSI color codes | Same ANSI color codes (colored crate) + optional `--json` |
| **Bootstrap** | `uv run scripts/bootstrap_bmadder.py` | `bmadder bootstrap` |
| **Validation** | `uv run scripts/validate_stories.py` | `bmadder validate` |
| **Auth preflight** | `uv run scripts/preflight_auth.py` | Built-in to phase startup |

---

## 21. Appendix A: Complete `bmadder.toml` Reference

```toml
# =============================================================================
# bmadder.toml — BMADder Orchestrator Configuration
# =============================================================================
# All paths are relative to the directory containing this file.
# Edit freely. Changes take effect on next `bmadder` invocation.
# =============================================================================

[paths]
skills_dir            = ".agent/skills"
headless_dir          = "scripts/headless-skills"
stories_dir           = "docs/backlog/stories"
state_dir             = "_bmad"
prd_file              = "docs/prd.md"
architecture_file     = "docs/architecture.md"
orchestrator_marker   = "_bmad/orchestrator-master.md"

[models]
sonnet      = "claude-sonnet-4"
opus        = "claude-opus-4"
gpt5        = "gpt-5"
gemini_pro  = "gemini-2.5-pro"
codex_def   = "codex-default"

[roles.sm]
personality = "bmad-agent-dev"
model       = "sonnet"
headless    = "sm-create-stories.md"

[roles.sm_single]
personality = "bmad-agent-dev"
model       = "sonnet"
headless    = "sm-create-story.md"

[roles.po]
personality = "bmad-agent-pm"
model       = "sonnet"
headless    = "po-review.md"

[roles.dev]
personality = "bmad-agent-dev"
model       = "gpt5"
headless    = "dev-story.md"

[roles.qa]
personality = "bmad-agent-dev"
model       = "sonnet"
headless    = "qa-review.md"

[agent_hints]
codex  = "gpt5"
claude = "sonnet"
gemini = "gemini_pro"

[defaults]
max_dev_iterations       = 10
max_sm_iterations        = 5
max_qa_passes            = 3
story_timeout_seconds    = 1800
gemini_cooldown_seconds  = 15
gemini_initial_backoff   = 30

[pi_dev]
command = "pi.dev"
args = [
    "--model",        "{model}",
    "--personality",  "{personality}",
    "--instructions", "{headless}",
    "--task",         "@{prompt_file}",
    "--workspace",    "{workspace}",
    "--timeout",      "{timeout}",
    "--json-output"
]
```

---

## 22. Appendix B: Headless Skill Reference

| File | Role | Description | Source Skills (in `.agent/skills/`) |
|---|---|---|---|
| `sm-create-stories.md` | Scrum Master (batch) | Shard PRD into all story files | `bmad-create-epics-and-stories/*`, `bmad-create-story/checklist.md`, `bmad-create-story/template.md` |
| `sm-create-story.md` | Scrum Master (single) | Create one story file | `bmad-create-story/workflow.md`, `bmad-create-story/discover-inputs.md`, `bmad-create-story/checklist.md`, `bmad-create-story/template.md` |
| `po-review.md` | Product Owner | Story quality review gate | `bmad-create-story/checklist.md` |
| `dev-story.md` | Developer | TDD implementation workflow | `bmad-dev-story/workflow.md` |
| `qa-review.md` | QA Auditor | Code review & acceptance verification | `bmad-code-review/workflow.md`, `bmad-code-review/steps/step-01-*.md` through `step-04-*.md` |

---

## 23. Appendix C: BMAD Agent Personality Reference

These are the SKILL.md files in `.agent/skills/` that serve as `pi.dev --personality` arguments. Each defines a named persona with identity, communication style, and principles.

| Directory | Name | Role | Capabilities |
|---|---|---|---|
| `bmad-agent-dev` | Amelia | Senior Software Engineer | Story execution, test-driven development, code implementation |
| `bmad-agent-pm` | John | Product Manager | PRD creation, requirements discovery, stakeholder alignment |
| `bmad-agent-architect` | Winston | System Architect | Distributed systems, cloud infrastructure, API design, scalable patterns |
| `bmad-agent-analyst` | Mary | Business Analyst | Market research, competitive analysis, requirements elicitation |
| `bmad-agent-tech-writer` | Paige | Technical Writer | Documentation, Mermaid diagrams, standards compliance |
| `bmad-agent-ux-designer` | Sally | UX Designer | User research, interaction design, UI patterns |

Each SKILL.md follows this structure:

```markdown
---
name: bmad-agent-dev
description: Senior software engineer for story execution and code implementation.
---

# Amelia

## Overview
This skill provides a Senior Software Engineer who executes approved stories...

## Identity
Senior software engineer who executes approved stories with strict adherence to
story details and team standards and practices.

## Communication Style
Ultra-succinct. Speaks in file paths and AC IDs — every statement citable.

## Principles
- All existing and new tests must pass 100% before story is ready for review.
- Every task/subtask must be covered by comprehensive unit tests before marking complete.
```

---

*Document version: 1.0*
*Last updated: 2026-06-18*
