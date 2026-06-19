# BMADDer Framework

Autonomous AI-driven software development. Feed it a PRD and architecture doc, get back a working MVP.

BMADDer is a Rust binary that orchestrates a BMAD state machine. It cycles through story creation (SM), product review (PO gate), development (TDD with build/test/lint feedback), and QA (deep code review) — all using `pi --skill` agent invocations with fresh context per call. No conversation drift, no hidden state. The filesystem is the memory.

## How It Works

```
Idea
  │
  ▼
PRD + Architecture docs
  │
  ▼
bmadder bootstrap          ← creates folder structure + bmadder.toml
  │
  ▼
┌─────────────────────────────────────────────────┐
│  PLAN PHASE                                     │
│                                                 │
│  SM: PRD + architecture ──→ atomic stories      │
│  PO: review all drafts ──→ approve / revise     │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────┐
│  DEV PHASE — per story                          │
│                                                 │
│  Fresh context each iteration                   │
│  TDD: failing tests ──→ implement ──→ pass      │
│  Feedback loops: build + test + lint            │
│  Max 10 iterations per story                    │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────┐
│  QA PHASE — per story                           │
│                                                 │
│  Deep review vs acceptance criteria             │
│  PASS ──→ COMPLETED + git commit + push         │
│  FAIL ──→ REFIX ──→ back to Dev                 │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
              REFIX loop (max 3 passes)
                       │
                       ▼
                      MVP
```

## Philosophy

**Fresh context per invocation.** Every agent call starts clean. No conversation history. Agents discover prior work by reading `progress.txt`, `git log`, story frontmatter, and Implementation Notes. This prevents hallucination drift and makes every invocation independently reproducible.

**Rust is the enforcer.** The LLM does work within guardrails — it never decides workflow. The Rust runtime reads frontmatter on disk, validates state transitions, and decides what to invoke next. If an agent claims it's done but didn't update the story file, the runtime catches it.

**Sequential story processing.** One story at a time, in dependency order. Parallel execution sounds nice but creates merge conflicts and cross-story contamination. Sequential is boring and correct.

**Filesystem is memory.** `progress.txt` is the append-only dev log. `activity.log` is the structured audit trail. Story frontmatter is the state machine. Git history is the source of truth. No database, no service, no hidden state.

**TDD is mandatory.** Write failing tests first, then implement until they pass. Run build/test/lint feedback loops before marking done. The dev agent doesn't get to skip this.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs) — to build the binary
- [pi](https://pi.dev) — agent CLI used for all skill invocations
- git
- At least one model configured in your `bmadder.toml` (see Configuration)

### 1. Build the binary

```bash
cargo build --release
# Binary lands at ./target/release/bmadder
```

### 2. Bootstrap a new project

```bash
bmadder bootstrap /path/to/your-project
```

This creates the folder structure, generates `bmadder.toml`, initializes git, and checks tooling.

### 3. Add your PRD and architecture

Fill in `docs/prd.md` and `docs/architecture.md`. These are the inputs that drive everything. They need to be specific enough that an agent can decompose them into implementable stories.

### 4. Configure `bmadder.toml`

Edit the generated config to point at your skill directories and set your models. See [Configuration](#configuration) below.

### 5. Run the full cycle

```bash
bmadder cycle
```

Or run individual phases:

```bash
bmadder plan       # SM creates stories, PO reviews
bmadder dev        # Dev loop, one story at a time
bmadder qa         # QA audit, one story at a time
bmadder status     # Show story states
bmadder validate   # Check story frontmatter only
bmadder ui         # Launch the browser console
```

## Configuration

`bmadder.toml` lives at the project root and is auto-discovered on startup.

```toml
[paths]
skills_dir = ".agent/skills"          # pi --skill directories
stories_dir = "docs/backlog/stories"  # story markdown files
state_dir = "_bmad"                   # progress + activity logs

[models]
sonnet = "claude-sonnet-4"
opus   = "claude-opus-4"
gpt5   = "gpt-5"

[roles.sm]
personality = "bmad-agent-dev"
model       = "sonnet"
skill       = "bmad-create-epics-and-stories"

[roles.po]
personality = "bmad-agent-dev"
model       = "sonnet"
skill       = "bmad-create-epics-and-stories"

[roles.dev]
personality = "bmad-agent-dev"
model       = "gpt5"
skill       = "bmad-dev-story"

[roles.qa]
personality = "bmad-agent-dev"
model       = "sonnet"
skill       = "bmad-code-review"

[agent_hints]
codex  = "gpt5"
claude = "sonnet"

[defaults]
max_dev_iterations      = 10
max_sm_iterations       = 5
max_qa_passes           = 3
story_timeout_seconds   = 1800
gemini_cooldown_seconds = 15
gemini_initial_backoff  = 30

[pi_dev]
command = "pi"
args    = ["--model", "{model}", "--skill", "{skill}", "--print", "--mode", "json", "--no-session", "--approve"]
```

## Commands

```
bmadder [options] <command>
```

### Subcommands

| Command | What it does |
|---------|-------------|
| `bootstrap [dir]` | Set up a new project (default: current directory) |
| `plan` | SM shards PRD → stories, PO reviews and approves |
| `dev` | Sequential dev loop, one story at a time, fresh context |
| `qa` | Sequential QA audit, one story at a time, fresh context |
| `cycle` | Full pipeline: plan → dev → qa (loops on REFIX, max 3 passes) |
| `iterative` | Story-at-a-time lifecycle: plan then immediately dev+qa each story |
| `status` | Show current story states and key file status |
| `validate` | Validate story frontmatter against the state machine |
| `ui` | Serve the browser console at `http://127.0.0.1:7331` |

### Global Options

| Option | Default | Description |
|--------|---------|-------------|
| `--config <path>` | auto-discovered | Path to `bmadder.toml` |
| `--max-iter N` | 10 | Max dev iterations per story |
| `--max-sm-iter N` | 5 | Max SM↔PO revision cycles |
| `--max-dev-iter N` | 10 | Max dev iterations (alias) |
| `--dry-run` | — | Show what would run without executing |
| `--skip-po` | — | Skip PO gate, auto-approve all drafts |
| `--skip-sm` | — | Skip SM story creation |
| `--agent KEY` | — | Force all phases to use this model key |
| `--no-commit` | — | Skip git commit/push after QA pass |
| `--timeout SECS` | 1800 | Max seconds per agent invocation |
| `--story ID` | — | Target a specific story (e.g. `STORY-0001`) |
| `--from-existing` | — | Resume from existing stories (iterative mode) |
| `--start-from ID` | — | Start iterative run from a specific story |
| `--json` | — | Output status as JSON |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `BMADDER_AGENT` | Force all phases to one model key |
| `BMADDER_MAX_ITER` | Max dev iterations per story |
| `BMADDER_MAX_SM_ITER` | Max SM iterations |
| `BMADDER_MAX_DEV_ITER` | Max dev iterations |
| `BMADDER_STORY_TIMEOUT` | Max seconds per agent invocation |
| `BMADDER_PLAN_AGENT` | Model key for plan phase |
| `BMADDER_DEV_AGENT` | Model key for dev phase |
| `BMADDER_QA_AGENT` | Model key for QA phase |

## State Machine

Stories move through a strict state machine. Only specific roles can make each transition.

```
DRAFT ──→ REVISE ──→ DRAFT          SM/PO revision loop
DRAFT ──→ READY_FOR_DEV             PO approves
READY_FOR_DEV ──→ IN_DEV            Orchestrator assigns to dev
IN_DEV ──→ PENDING_QA               Dev completes, tests pass
PENDING_QA ──→ COMPLETED            QA passes
PENDING_QA ──→ REFIX                QA fails
REFIX ──→ IN_DEV                    Back to dev for fixes
```

| Transition | Who | Gate |
|-----------|-----|------|
| DRAFT → READY_FOR_DEV | PO only | `po_alignment` must be `APPROVED` |
| DRAFT → REVISE | PO only | Needs revision notes |
| REVISE → DRAFT | SM | SM addresses PO feedback |
| READY_FOR_DEV → IN_DEV | Orchestrator | Automatic at dev start |
| IN_DEV → PENDING_QA | Dev only | Build + test + lint must pass |
| PENDING_QA → COMPLETED | QA only | All acceptance criteria verified |
| PENDING_QA → REFIX | QA only | Failed criteria documented |
| REFIX → IN_DEV | Orchestrator | Automatic at refix start |

## Agent Routing

Stories carry an `agent_hint` field in their frontmatter that selects the dev model.

| Phase | Role key | Default model key |
|-------|----------|------------------|
| Plan (SM) | `sm` | `sonnet` |
| Plan (PO) | `po` | `sonnet` |
| Dev | `dev` | per `agent_hints` map or role default |
| QA | `qa` | `sonnet` |

`agent_hint` values in stories are looked up in `[agent_hints]` in `bmadder.toml`, then resolved to a model string via `[models]`. Override routing:

```bash
bmadder cycle --agent claude        # force all phases to the "claude" model key
BMADDER_DEV_AGENT=gpt5 bmadder dev  # env override for dev phase only
```

## Story Format

Stories live in `docs/backlog/stories/` as markdown with YAML frontmatter.

**Filename:** `story-NNNN-slug.md`

**Frontmatter:**

```yaml
---
story_id: "STORY-0012"
epic_id: "EPIC-0003"
title: "Implement user authentication"
status: "DRAFT"
priority: "MUST_HAVE"
agent_hint: "codex"
assigned_dev: null
po_alignment: "PENDING"
qa_status: "NOT_STARTED"
created_at: "2026-03-14"
updated_at: "2026-03-14"
links: []
---
```

| Field | Valid values |
|-------|-------------|
| `status` | `DRAFT` `REVISE` `READY_FOR_DEV` `IN_DEV` `PENDING_QA` `REFIX` `COMPLETED` |
| `priority` | `MUST_HAVE` `SHOULD_HAVE` `COULD_HAVE` `WONT_HAVE` |
| `agent_hint` | any key defined in `[agent_hints]` in `bmadder.toml` |
| `po_alignment` | `PENDING` `APPROVED` `REVISE` |
| `qa_status` | `NOT_STARTED` `PASS` `FAIL` |

**Required sections:**

```markdown
## Context
## Requirements
## Acceptance Criteria
## Implementation Notes
## PO Alignment
## QA Notes
```

## Project Structure

```
bmadder-framework/
├── Cargo.toml                    ← Rust workspace
├── Cargo.lock
├── bmadder-cli/                  ← Binary crate (bmadder)
│   └── src/
│       ├── main.rs
│       ├── bootstrap.rs
│       ├── agent.rs
│       ├── git.rs
│       ├── ui.rs
│       ├── story_io.rs
│       └── phases/
│           ├── plan.rs
│           ├── dev.rs
│           ├── qa.rs
│           ├── cycle.rs
│           ├── iterative.rs
│           ├── status.rs
│           └── validate.rs
├── bmadder-core/                 ← Library crate (config, story types)
│   └── src/
│       ├── config.rs
│       ├── story.rs
│       └── agent.rs
├── .agent/
│   └── skills/                   ← pi --skill directories
│       ├── bmad-create-epics-and-stories/
│       ├── bmad-dev-story/
│       ├── bmad-code-review/
│       └── ...
├── _bmad/                        ← Runtime state (gitignored logs)
│   ├── orchestrator-master.md
│   ├── progress.txt
│   └── logs/
│       └── activity.log
├── docs/
│   ├── prd.md
│   ├── architecture.md
│   └── backlog/
│       └── stories/
├── ui/
│   ├── BMADder Console.dc.html   ← Browser console (embedded in binary)
│   └── screenshots/
├── scripts/
│   └── deploy-push.sh            ← Build + Docker + GHCR deploy
└── _deprecated/                  ← Shell-era scripts (archived, not used)
```

### Bootstrapped Project Layout

When `bmadder bootstrap` runs on a new project, it creates:

```
your-project/
├── bmadder.toml                  ← Configuration
├── .gitignore
├── .agent/
│   └── skills/                   ← copy or symlink your skills here
├── _bmad/
│   ├── orchestrator-master.md
│   ├── progress.txt
│   └── logs/
│       └── activity.log
├── docs/
│   ├── prd.md                    ← fill this in
│   ├── architecture.md           ← fill this in
│   └── backlog/
│       └── stories/
└── src/                          ← your code goes here
```

## Auth & Billing

BMADDer invokes agents via `pi --skill`. Auth is managed by `pi`'s own credential store — no rogue API key env vars needed. The Rust preflight check at startup verifies that `pi` is on PATH and warns if `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, or `GOOGLE_API_KEY` are set, as these can silently switch CLIs from subscription to per-token billing.

## Browser Console

```bash
bmadder ui                        # http://127.0.0.1:7331
bmadder ui --host 0.0.0.0 --port 8080
```

The console embeds `ui/BMADder Console.dc.html` directly in the binary at compile time. It exposes:

- `GET /api/status` — config, paths, model/role map, story counts
- `GET /api/stories` — full story list with frontmatter and AC progress
- `GET /api/logs/activity` — activity log (last 200 entries)
- `POST /api/run` — spawn a `bmadder` subcommand from the UI

## Deployment

```bash
./scripts/deploy-push.sh [tag]
```

Builds the release binary, packages it into a Docker image, pushes to GHCR, and deploys.

## License

MIT. See [LICENSE](LICENSE).
