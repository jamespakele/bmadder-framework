# BMADDer Framework

Autonomous AI-driven software development. Feed it a PRD and architecture doc, get back a working MVP.

BMADDer is a Ralph Wiggum loop with BMAD state machine gates. A bash orchestrator cycles through story creation (SM), product review (PO gate), development (TDD with build/test/lint feedback), and QA (deep code review) — all using CLI agents with fresh context per invocation. No conversation drift, no hidden state. The filesystem is the memory.

## How It Works

```
Idea
  │
  ▼
Perplexity / Manual ──→ PRD + Architecture docs
  │
  ▼
Google Stitch (optional) ──→ Design tokens + scaffolding
  │
  ▼
Bootstrap ──→ uv run scripts/bootstrap_bmadder.py
  │
  ▼
┌─────────────────────────────────────────────────┐
│  PLAN PHASE — Claude Sonnet                     │
│                                                 │
│  SM: PRD + architecture ──→ atomic stories      │
│  PO: review all drafts ──→ approve / revise     │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────┐
│  DEV PHASE — Codex / Claude / Gemini per story  │
│                                                 │
│  Fresh context each iteration                   │
│  TDD: failing tests ──→ implement ──→ pass      │
│  Feedback loops: build + test + lint            │
│  Max 10 iterations per story                    │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────┐
│  QA PHASE — Claude Opus                         │
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

**Bash is the enforcer.** The LLM does work within guardrails — it never decides workflow. The bash script reads frontmatter on disk, validates state transitions, and decides what to invoke next. If an agent claims it's done but didn't update the story file, bash catches it.

**Sequential story processing.** One story at a time, in dependency order. Parallel execution sounds nice but creates merge conflicts and cross-story contamination. Sequential is boring and correct.

**Filesystem is memory.** `progress.txt` is the append-only dev log. `activity.log` is the structured audit trail. Story frontmatter is the state machine. Git history is the source of truth. No database, no service, no hidden state.

**TDD is mandatory.** Write failing tests first, then implement until they pass. Run build/test/lint feedback loops before marking done. The dev agent doesn't get to skip this.

## Quick Start

### Prerequisites

- [mise](https://mise.run) — tool version manager
- [uv](https://github.com/astral-sh/uv) — fast Python package manager
- git
- At least one agent CLI:
  - [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (`claude`) — needed for plan + QA phases
  - [Codex CLI](https://github.com/openai/codex) (`codex`) — default dev agent
  - [Gemini CLI](https://github.com/google-gemini/gemini-cli) (`gemini`) — optional, UI-only

### 1. Create your project

```bash
mkdir my-project && cd my-project
```

### 2. Copy the framework scripts

```bash
# Clone or copy bmadder-framework into your project
cp -r /path/to/bmadder-framework/scripts ./scripts
```

### 3. Bootstrap

```bash
uv run scripts/bootstrap_bmadder.py
```

This creates the full folder structure, generates the orchestrator contract and standards files, synchronizes/freshens headless skills (converting raw interactive BMad skills from `.agent/skills/` into non-interactive Markdown files under `scripts/headless-skills/`), initializes git, and checks your tooling.

### 4. Add your PRD and architecture

Fill in `docs/prd.md` and `docs/architecture.md`. These are the inputs that drive everything. Generate them with Perplexity, write them by hand, or use whatever works. They need to be specific enough that an agent can decompose them into implementable stories.

### 5. Run the auth preflight

```bash
uv run scripts/preflight_auth.py
```

Verifies your agent CLIs are installed, authenticated, and not accidentally billing to API keys instead of subscriptions.

### 6. Run the full cycle

Pick between the batch pipeline or the story-by-story iterative pipeline:

```bash
# Option A: Batch Mode (plan all -> dev all -> qa all)
./scripts/bmadder.sh cycle

# Option B: Iterative Mode (plan -> per-story dev/qa lifecycle)
./scripts/bmadder-iterative.sh cycle
```

This runs the selected orchestrator cycle. Check `./scripts/bmadder.sh status` (or `./scripts/bmadder-iterative.sh status`) to see where things stand.

### 7. Run individual phases

```bash
./scripts/bmadder.sh plan       # SM + PO only
./scripts/bmadder.sh dev        # Dev loop only
./scripts/bmadder.sh qa         # QA audit only
./scripts/bmadder.sh status     # Show story states
./scripts/bmadder.sh validate   # Check frontmatter only
```

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

**Transition rules:**

| Transition | Who | Gate |
|-----------|-----|------|
| DRAFT → READY_FOR_DEV | PO only | po_alignment must be APPROVED |
| DRAFT → REVISE | PO only | Needs revision notes |
| REVISE → DRAFT | SM | SM addresses PO feedback |
| READY_FOR_DEV → IN_DEV | Orchestrator | Automatic at dev start |
| IN_DEV → PENDING_QA | Dev only | Build + test + lint must pass |
| PENDING_QA → COMPLETED | QA only | All acceptance criteria verified |
| PENDING_QA → REFIX | QA only | Failed criteria documented |
| REFIX → IN_DEV | Orchestrator | Automatic at refix start |

## Agent Routing

The orchestrator picks the right agent for each phase. Stories carry an `agent_hint` field that routes the dev agent.

| Phase | Default Agent | Model | Rationale |
|-------|--------------|-------|-----------|
| Plan (SM) | claude | sonnet | Structured reasoning, doc generation |
| Plan (PO) | claude | sonnet | Checklist verification, cross-story review |
| Dev (backend) | codex | — | Long-horizon coding, strong TDD compliance |
| Dev (complex logic) | claude | sonnet | Data transforms, config, cross-module logic |
| Dev (UI/UX) | gemini | — | Multimodal, but rarely used (needs Stitch scaffolding) |
| QA | claude | opus | Deep code review, nuanced quality decisions |

**agent_hint values:**

- `codex` — backend, API, database, infrastructure, AND most frontend. This is the default for all stories.
- `claude` — complex logic, data transforms, config, cross-module dependencies.
- `gemini` — only if no Stitch scaffolding exists and you need multimodal UI generation. Rare.

**Override routing:**

```bash
# Force all phases to one agent
./scripts/bmadder.sh cycle --agent claude

# Environment variable overrides
BMADDER_PLAN_AGENT=claude   # Plan phase (default: claude)
BMADDER_DEV_AGENT=codex     # Dev phase (default: codex)
BMADDER_QA_AGENT=claude     # QA phase (default: claude, uses opus)
BMADDER_AGENT=claude        # Force ALL phases
```

## Story Format

Stories live in `docs/backlog/stories/` as markdown with YAML frontmatter.

**Filename:** `story-NNNN-slug.md` — NNNN encodes priority/dependency order.

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

**Valid values:**

| Field | Values |
|-------|--------|
| status | `DRAFT` `REVISE` `READY_FOR_DEV` `IN_DEV` `PENDING_QA` `REFIX` `COMPLETED` |
| priority | `MUST_HAVE` `SHOULD_HAVE` `COULD_HAVE` `WONT_HAVE` |
| agent_hint | `codex` `claude` `gemini` |
| po_alignment | `PENDING` `APPROVED` `REVISE` |
| qa_status | `NOT_STARTED` `PASS` `FAIL` |

**Required sections:**

```markdown
## Context
## Requirements
## Acceptance Criteria
## Implementation Notes
## PO Alignment
## QA Notes
```

See `.deprecated/templates/story-template.md` for a legacy story template. Stories are normally created dynamically by the Scrum Master planning skill.

## Commands

```
./scripts/bmadder.sh [phase] [options]
```

### Phases

| Phase | What it does |
|-------|-------------|
| `plan` | SM shards PRD → stories, PO reviews all at once |
| `dev` | Sequential dev loop, one story at a time, fresh context |
| `qa` | Sequential QA audit, one story at a time, fresh context |
| `cycle` | Full pipeline: plan → dev → qa (loops on REFIX, max 3 passes) |
| `status` | Show current story states and key file status |
| `validate` | Run story frontmatter validation only |

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--max-iter N` | 10 | Max dev iterations per story before stalling |
| `--dry-run` | — | Show what would run without executing |
| `--skip-po` | — | Skip PO gate, auto-approve all drafts (rapid prototyping only) |
| `--agent AGENT` | — | Force ALL phases to use this agent |
| `--no-commit` | — | Skip git commit/push after QA pass |
| `--timeout SECS` | 1800 | Max seconds per agent invocation |
| `--story ID` | — | Target a specific story (e.g., `STORY-0001`) |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BMADDER_AGENT` | — | Force all phases to one agent |
| `BMADDER_MAX_ITER` | 10 | Max dev iterations per story |
| `BMADDER_STORY_TIMEOUT` | 1800 | Max seconds per agent invocation |
| `BMADDER_PLAN_AGENT` | claude | Plan phase agent |
| `BMADDER_DEV_AGENT` | codex | Dev phase default agent |
| `BMADDER_QA_AGENT` | claude | QA phase agent (uses opus model) |

## Auth & Billing

BMADDer is designed for **subscription-based** agent CLIs (Claude Pro/Max, ChatGPT Plus, Google AI). The preflight check catches environment variables that silently switch to per-token API billing:

| Agent | Rogue Variable | What Happens |
|-------|---------------|-------------|
| Claude | `ANTHROPIC_API_KEY` | Switches from Pro/Max subscription to per-token API billing |
| Codex | `OPENAI_API_KEY` | Switches from ChatGPT Plus to per-token API billing |
| Gemini | `GEMINI_API_KEY` / `GOOGLE_API_KEY` | Switches from subscription to per-token API billing |

A full cycle can run 50+ agent invocations. On API billing, that gets expensive fast. The preflight check warns you before it happens.

```bash
# Check for issues
uv run scripts/preflight_auth.py

# Auto-fix for this session (unsets rogue vars)
uv run scripts/preflight_auth.py --fix

# Check specific agents only
uv run scripts/preflight_auth.py --agents claude codex
```

## File Reference

### Framework Files (from this package)

| File | Purpose |
|------|---------|
| `scripts/bmadder.sh` | Batch orchestrator — state machine, agent routing, phase execution (plan all → dev all → qa all) |
| `scripts/bmadder-iterative.sh` | Iterative orchestrator — story-at-a-time lifecycle execution (plan all → per-story dev/qa lifecycle) |
| `scripts/sync_headless_skills.py` | Headless skill generator — strips interactivity and consolidates source BMAD agent skills |
| `scripts/bootstrap_bmadder.py` | One-command project setup (calls init, create_rules, sync_headless_skills) |
| `scripts/init_bmadder.py` | Creates folder structure (called by bootstrap) |
| `scripts/create_rules.py` | Generates orchestrator contract and standards (called by bootstrap) |
| `scripts/validate_stories.py` | Validates story frontmatter against state machine |
| `scripts/preflight_auth.py` | Verifies agent auth and catches rogue API keys |

### Project Files (created by bootstrap)

| File | Purpose |
|------|---------|
| `_bmad/orchestrator-master.md` | Governing contract — state machine, roles, conventions |
| `_bmad/progress.txt` | Append-only dev progress log |
| `_bmad/logs/activity.log` | Structured activity log |
| `_bmad/.prompt-tmp.md` | Temp file for agent prompts (gitignored) |
| `docs/prd.md` | Product Requirements Document |
| `docs/architecture.md` | Architecture Document |
| `docs/backlog/stories/` | Story files with YAML frontmatter |
| `docs/backlog/epics/` | Epic files |
| `docs/standards/scrum-master-guide.md` | SM instructions for story creation |
| `docs/standards/po-alignment-checklist.md` | PO review checklist |
| `docs/standards/qa-standards.md` | QA verification standards |

### Optional Files

| File | Purpose |
|------|---------|
| `src/scaffolding/tokens.md` | Design tokens from Stitch export |
| `src/scaffolding/layouts/` | Page layout templates from Stitch |
| `src/scaffolding/components/` | UI component templates from Stitch |
| `.mise.toml` | Tool version config (created by bootstrap) |

## Script Details

Detailed documentation for each script, including options not covered in the Commands section above.

### `bootstrap_bmadder.py` — One-Command Setup

Runs everything needed to set up a BMADder project:
1. Creates folder structure (`init_bmadder.py`)
2. Generates orchestrator + standards files (`create_rules.py`)
3. Creates `.mise.toml` and `.gitignore`
4. Verifies required tools (mise, uv, git) and optional tools (claude, codex, gemini, cargo)
5. Initializes git repo if needed
6. Checks for PRD and architecture docs
7. Synchronizes headless skills by running `sync_headless_skills.py` (which processes raw interactive BMAD skills from `.agent/skills/` into consolidated non-interactive Markdown files under `scripts/headless-skills/`).

```bash
uv run scripts/bootstrap_bmadder.py          # interactive
uv run scripts/bootstrap_bmadder.py --auto   # non-interactive (CI/scripts)
```

### `init_bmadder.py` — Folder Structure

Creates the standard BMADder directory layout. Safe to re-run. Seeds `docs/prd.md` and `docs/architecture.md` with templates if they don't exist.

### `create_rules.py` — Rule File Generator

Generates the framework governance files. Skips files that already exist:

- `_bmad/orchestrator-master.md` — agent contract, state machine, story spec
- `docs/standards/scrum-master-guide.md` — SM sharding rules
- `docs/standards/po-alignment-checklist.md` — PO review questions
- `docs/standards/qa-standards.md` — QA audit checklist

### `validate_stories.py` — Story Validator

Validates all story files in `docs/backlog/stories/` against the BMADder spec:

- **Frontmatter checks:** required fields, valid status/po_alignment/qa_status/agent_hint values
- **Consistency checks:** e.g., READY_FOR_DEV requires po_alignment=APPROVED
- **Section checks:** all 6 required sections present (Context, Requirements, Acceptance Criteria, Implementation Notes, PO Alignment, QA Notes)
- **Filename convention:** must match `story-NNNN-slug.md`

```bash
uv run scripts/validate_stories.py           # validate only
uv run scripts/validate_stories.py --fix     # auto-insert missing sections, then validate
```

The `--fix` mode inserts stub sections in the correct canonical order for any missing required sections. It will not overwrite existing content.

### `preflight_auth.py` — Auth & Billing Safety

Pre-flight checks before running agent invocations:

1. **Billing safety** — Detects rogue env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`) that silently switch CLIs from subscription to per-token API billing
2. **CLI installed** — Verifies each agent binary is in PATH
3. **Auth live** — Best-effort check that each agent is authenticated

```bash
uv run scripts/preflight_auth.py                         # check all agents
uv run scripts/preflight_auth.py --agents claude codex   # check specific ones
uv run scripts/preflight_auth.py --fix                   # unset rogue env vars for session
```

## Integration with Perplexity Computer

The ideal input pipeline: use Perplexity (or any research tool) to brainstorm and generate your PRD and architecture docs. The quality of BMADDer's output is directly proportional to the quality of these inputs. Vague PRD → vague stories → vague code.

What makes a good PRD for BMADDer:
- Specific, numbered functional requirements
- Clear non-functional requirements (performance, security)
- Explicit out-of-scope section
- User personas with real scenarios

What makes a good architecture doc:
- Concrete tech stack decisions (not "we could use X or Y")
- Data model with actual entities and relationships
- API endpoints with request/response shapes
- Development conventions (test framework, code style, git workflow)

## Integration with Google Stitch

For projects with UI, run the Stitch design gate before coding. Stitch generates design artifacts that agents reference during frontend development. See [docs/DESIGN-GATE.md](docs/DESIGN-GATE.md) for the full workflow.

## Known Issues & Lessons Learned

See [docs/LESSONS-LEARNED.md](docs/LESSONS-LEARNED.md) for battle-tested knowledge from real project runs, and [docs/AGENT-COMPATIBILITY.md](docs/AGENT-COMPATIBILITY.md) for agent CLI quirks and workarounds.

## Project Structure

```
your-project/
├── scripts/
│   ├── bmadder.sh                ← Batch orchestrator
│   ├── bmadder-iterative.sh      ← Iterative orchestrator
│   ├── bootstrap_bmadder.py      ← One-command setup
│   ├── init_bmadder.py           ← Folder structure creator
│   ├── create_rules.py           ← Rules/standards generator
│   ├── validate_stories.py       ← Frontmatter validator
│   ├── preflight_auth.py         ← Auth/billing safety check
│   ├── sync_headless_skills.py   ← Headless skill generator
│   └── headless-skills/          ← Consolidates non-interactive MD skills
├── docs/
│   ├── prd.md                    ← Your product requirements
│   ├── architecture.md           ← Your system design
│   ├── backlog/
│   │   ├── epics/                ← Epic files
│   │   └── stories/              ← Story files (YAML frontmatter)
│   └── standards/
│       ├── scrum-master-guide.md
│       ├── po-alignment-checklist.md
│       └── qa-standards.md
├── _bmad/                        ← Core framework directory
│   ├── orchestrator-master.md    ← Governing contract
│   ├── progress.txt              ← Append-only dev log
│   └── logs/
│       └── activity.log          ← Structured activity log
├── src/                          ← Your code goes here
│   └── scaffolding/              ← (optional) Stitch design artifacts
└── .mise.toml
```

## Modifications Log

### 2026-03-14

#### `bmadder.sh`
- **Default timeout increased from 900s (15 min) to 1800s (30 min).**
  The SM planning agent was consistently hitting the 15-minute timeout when decomposing large PRDs (77 stories across 11 epics). The agent was actively writing story files when killed. 30 minutes provides adequate headroom for complex projects.

#### `bootstrap_bmadder.py`
- **Added `PermissionError` to the exception handler in `check_tool()`.**
  When `.mise.toml` isn't trusted, tool shims exist but aren't executable, causing Python to raise `PermissionError` instead of `FileNotFoundError`. The script now catches this gracefully and reports `[MISS]` instead of crashing the entire bootstrap.

#### `validate_stories.py`
- **Added `--fix` mode to auto-insert missing required sections.**
  After the SM agent was killed by timeout and re-ran, 13 stories (0091–0103) were created without `## PO Alignment` sections, and 2 of those (0102–0103) were also missing `## Implementation Notes`. Root cause: the first SM run created partial files before the timeout, and the second run batch-wrote replacement files using a template that omitted the PO Alignment header. The `--fix` flag inserts stub sections in the canonical order defined by `orchestrator-master.md`, placing each before the next existing section. It does not modify existing content.

## License

MIT. See [LICENSE](LICENSE).
