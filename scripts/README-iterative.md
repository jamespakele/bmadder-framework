# BMADDer Framework — Iterative Mode

Autonomous AI-driven software development, one story at a time. Feed it a PRD and architecture doc, get back a working MVP — with a deployable, testable version after every single story.

BMADDer Iterative is a variant of the BMADDer orchestrator that follows the original BMAD method intention: the Scrum Master produces stories, the Product Owner reviews them, then **each story goes through its full lifecycle** (Dev → QA → REFIX loop → COMPLETED) before the next story begins. This produces incremental, working, deployable versions of your app as each story completes, with git rollback points at every milestone.

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
│  ► GIT COMMIT: "PO approved N stories"          │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
        ┌──────────────────────────────────┐
        │  PER-STORY LIFECYCLE             │
        │                                  │
        │  ┌────────────────────────────┐  │
        │  │ DEV: TDD, build/test/lint  │  │
        │  └───────────┬────────────────┘  │
        │              │                   │
        │              ▼                   │
        │  ┌────────────────────────────┐  │
        │  │ QA: deep review (Opus)     │  │
        │  │ PASS → COMPLETED           │  │
        │  │ FAIL → REFIX → back to Dev │  │
        │  └───────────┬────────────────┘  │
        │              │                   │
        │  REFIX loop (max 3 per story)    │
        │              │                   │
        │              ▼                   │
        │  ► GIT COMMIT: "STORY-NNNN       │
        │    [QA PASS]" — deployable!      │
        │                                  │
        └──────────────┬───────────────────┘
                       │
                  next story
                       │
                       ▼
                      MVP
```

## Batch vs Iterative — Side by Side

| Aspect | `bmadder.sh` (batch) | `bmadder-iterative.sh` (iterative) |
|--------|---------------------|-----------------------------------|
| **Plan** | SM creates all → PO reviews all | Same |
| **Dev** | All READY_FOR_DEV stories, then… | Per story: dev → QA → done |
| **QA** | …all PENDING_QA stories | Immediately after that story's dev |
| **REFIX** | Batch loop (dev all → qa all) × 3 | Per-story loop (dev → qa) × 3 |
| **Deployable after** | All stories pass QA | **Each** story passes QA |
| **Git commits** | After all QA passes | After PO approve + each QA pass |
| **Crash recovery** | Re-runs all incomplete work | Picks up at exact story, cleans orphaned code |

**Why iterative?** The original BMAD method envisions the SM handing stories to the developer one at a time, with QA checking each one. You get a working, testable version of the app after every single story — not just at the very end. If something goes wrong on story 15 of 40, you have 14 fully QA'd, committed stories to show for it.

## Philosophy

**Fresh context per invocation.** Every agent call starts clean. No conversation history. Agents discover prior work by reading `progress.txt`, `git log`, story frontmatter, and Implementation Notes. This prevents hallucination drift and makes every invocation independently reproducible.

**Bash is the enforcer.** The LLM does work within guardrails — it never decides workflow. The bash script reads frontmatter on disk, validates state transitions, and decides what to invoke next. If an agent claims it's done but didn't update the story file, bash catches it.

**Sequential story processing.** One story at a time, in dependency order. Parallel execution sounds nice but creates merge conflicts and cross-story contamination. Sequential is boring and correct.

**Filesystem is memory.** `progress.txt` is the append-only dev log. `activity.log` is the structured audit trail. Story frontmatter is the state machine. Git history is the source of truth. No database, no service, no hidden state.

**TDD is mandatory.** Write failing tests first, then implement until they pass. Run build/test/lint feedback loops before marking done. The dev agent doesn't get to skip this.

**Protect the codebase at all cost.** Git commits at two critical points (PO approval and QA pass) ensure the codebase is never in an unrecoverable state. If a dev agent gets quota-killed and leaves behind orphaned code, the orchestrator nukes it and rolls back to the last clean commit. An incomplete story can be redone — orphaned half-written code and broken imports are far more dangerous.

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

This creates the full folder structure, generates the orchestrator contract and standards files, initializes git, and checks your tooling.

### 4. Add your PRD and architecture

Fill in `docs/prd.md` and `docs/architecture.md`. These are the inputs that drive everything. Generate them with Perplexity, write them by hand, or use whatever works. They need to be specific enough that an agent can decompose them into implementable stories.

### 5. Run the auth preflight

```bash
uv run scripts/preflight_auth.py
```

Verifies your agent CLIs are installed, authenticated, and not accidentally billing to API keys instead of subscriptions.

### 6. Run the full iterative cycle

```bash
./scripts/bmadder-iterative.sh cycle
```

This runs plan → then per-story (dev → qa → refix loop). Each story reaches COMPLETED with a git commit before the next one starts. Check `./scripts/bmadder-iterative.sh status` to see where things stand.

### 7. Run individual phases

```bash
./scripts/bmadder-iterative.sh plan       # SM + PO only
./scripts/bmadder-iterative.sh dev        # Dev loop only
./scripts/bmadder-iterative.sh qa         # QA audit only
./scripts/bmadder-iterative.sh status     # Show story states
./scripts/bmadder-iterative.sh validate   # Check frontmatter only
```

## Git Commit Strategy

The iterative orchestrator commits at two critical checkpoints, creating safe rollback points throughout the entire process.

### Commit Points

```
1. PO Approval
   commit: "plan: PO approved 18 stories [pre-dev checkpoint]"
   ↓
   This is the LAST clean state before any dev agent touches code.
   If everything goes wrong, you can always roll back here.

2. Each QA Pass (per story)
   commit: "story(STORY-0003): Setup database schema [QA PASS]"
   ↓
   Code is verified, tested, QA-approved. Safe rollback point.

3. Each QA Pass (next story)
   commit: "story(STORY-0004): API endpoint scaffolding [QA PASS]"
   ↓
   ...and so on. Every commit is a deployable state.
```

### Why Two Commit Points?

**PO checkpoint (before dev starts):** Isolates the approved story backlog from any code changes. If a dev agent goes completely off the rails — writing to wrong files, breaking the project structure — you can `git reset --hard` back to the PO checkpoint and start dev over with a clean codebase. The stories, epics, and all planning work are preserved.

**QA PASS checkpoint (after each story):** Each completed story is a verified, tested increment. If story 5 breaks something that story 4 relied on, you can `git revert` just story 5's commit and keep stories 1-4 intact. The git history reads like a changelog:

```bash
$ git log --oneline
a3f8c21 story(STORY-0005): User auth middleware [QA PASS]
b7d2e14 story(STORY-0004): API endpoint scaffolding [QA PASS]
c1a9f37 story(STORY-0003): Setup database schema [QA PASS]
d4e5b68 plan: PO approved 18 stories [pre-dev checkpoint]
```

### Rollback Examples

```bash
# Roll back the last completed story
git revert HEAD

# Roll back to before any dev started (PO checkpoint)
git log --oneline --grep="pre-dev checkpoint"
git reset --hard <commit-hash>

# Roll back to after a specific story was completed
git log --oneline --grep="STORY-0003"
git reset --hard <commit-hash>
```

## Crash Recovery (Resume-on-Failure)

The most common failure mode is **agent quota limits** — codex or claude just stops mid-run when you hit your rate limit. Wait it out, then re-run `cycle`. The orchestrator picks up exactly where it left off.

### How It Works

The orchestrator reads story status from frontmatter **on disk** — no in-memory state to lose. On re-run, each story's frontmatter tag determines what happens:

| Story status at crash | On re-run | Reasoning |
|----------------------|-----------|-----------|
| `COMPLETED` | **Skipped** | Already done, nothing to do |
| `IN_DEV` | **Reset to READY_FOR_DEV** + git clean | Dev was interrupted. Discard orphaned code, start fresh. Stories are small — redo is cheap. |
| `PENDING_QA` | **Skip dev, redo QA** | Dev finished (code committed). QA just reads/checks — cheap to redo. |
| `REFIX` | **Run dev → QA** | QA failed, needs fixes. Normal flow. |
| `READY_FOR_DEV` | **Run dev → QA** | Normal flow. |
| `DRAFT` / `REVISE` | **Run plan first** | Stories need SM/PO attention before dev. |

### Why Reset IN_DEV Instead of Resuming?

When a dev agent gets quota-killed mid-coding, it can leave behind:
- Half-written files with incomplete functions
- Partial imports that break the build
- Orphaned test stubs
- Files in an inconsistent state

The next agent invocation starts with **fresh context** — it has no idea what the previous agent was thinking or where exactly it stopped. Trying to "resume" from dirty state is asking the new agent to reverse-engineer a dead agent's half-finished thought process.

Instead, the orchestrator:
1. Runs `git checkout -- . && git clean -fd` — **nukes all uncommitted changes**
2. Rolls back to the last clean commit (PO checkpoint or previous QA PASS)
3. Resets the story status to `READY_FOR_DEV`
4. Commits the status reset
5. Starts dev fresh with a clean codebase

The stories are designed to be small and atomic — redoing one is minutes, not hours. And any useful partial work the agent committed to git is still visible via `git log`, so the fresh agent can learn from it.

### Why Not Reset PENDING_QA?

If a story is at `PENDING_QA`, the dev agent successfully completed its work and committed. The code is safe in git. QA only **reads** code and writes notes to the story file — it doesn't modify the codebase. So even if QA crashes mid-review, there's nothing dangerous to clean up. Just redo QA.

### Resume Example

```bash
# First run — gets through 5 stories, then codex hits quota on story 6
./scripts/bmadder-iterative.sh cycle
# Output: 5 COMPLETED, 1 IN_DEV (quota killed), 12 READY_FOR_DEV

# Wait for quota to reset... then just re-run:
./scripts/bmadder-iterative.sh cycle
# Output:
#   ⚡ RESUMING from previous run:
#     5 COMPLETED (will skip)
#     1 IN_DEV → Reset to READY_FOR_DEV + discarded orphaned code
#     12 READY_FOR_DEV (will dev → QA)
#   Processing 13 stories iteratively...
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

**Iterative-specific transitions (crash recovery):**

| Transition | Who | Trigger |
|-----------|-----|---------|
| IN_DEV → READY_FOR_DEV | Orchestrator | Crash recovery: reset + git clean |
| PENDING_QA → PENDING_QA | Orchestrator | Crash recovery: redo QA |

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
./scripts/bmadder-iterative.sh cycle --agent claude

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
./scripts/bmadder-iterative.sh [phase] [options]
```

### Phases

| Phase | What it does |
|-------|-------------|
| `plan` | SM shards PRD → stories, PO reviews all at once, commits checkpoint |
| `dev` | Sequential dev loop, one story at a time, fresh context |
| `qa` | Sequential QA audit, one story at a time, fresh context |
| `cycle` | Iterative pipeline: plan → per-story (dev → qa → refix → COMPLETED) |
| `status` | Show current story states and key file status |
| `validate` | Run story frontmatter validation only |

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--max-iter N` | 10 | Max dev iterations per story before stalling |
| `--max-refix N` | 3 | Max QA refix passes per story (NEW in iterative) |
| `--dry-run` | — | Show what would run without executing |
| `--skip-po` | — | Skip PO gate, auto-approve all drafts (rapid prototyping only) |
| `--skip-sm` | — | Skip SM story creation, go straight to PO review (use when stories already exist) |
| `--agent AGENT` | — | Force ALL phases to use this agent |
| `--no-commit` | — | Skip git commit/push after QA pass |
| `--timeout SECS` | 1800 | Max seconds per agent invocation |
| `--story ID` | — | Target a specific story (e.g., `STORY-0001`) |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BMADDER_AGENT` | — | Force all phases to one agent |
| `BMADDER_MAX_ITER` | 10 | Max dev iterations per story |
| `BMADDER_MAX_REFIX` | 3 | Max QA refix passes per story |
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
| `scripts/bmadder.sh` | Original batch orchestrator (plan all → dev all → qa all) |
| `scripts/bmadder-iterative.sh` | **Iterative orchestrator (plan → per-story dev/qa lifecycle)** |
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

### `bmadder-iterative.sh` — Iterative Orchestrator

The story-at-a-time variant. Key internal functions:

| Function | Purpose |
|----------|---------|
| `run_plan()` | SM + PO, same as batch. Commits PO checkpoint after approval. |
| `run_dev_story()` | Dev loop for ONE story (max N iterations). |
| `run_qa_story()` | QA audit for ONE story. Commits + pushes on PASS. |
| `run_story_lifecycle()` | Core iterative loop: dev → qa → refix for ONE story. |
| `run_cycle()` | Full pipeline: plan (if needed) → per-story lifecycle. |

**Crash recovery in `run_cycle():`**
1. Detects `IN_DEV` stories (crashed mid-dev)
2. Runs `git checkout -- . && git clean -fd` to discard orphaned code
3. Resets status to `READY_FOR_DEV`
4. Commits the reset
5. Queues story for fresh dev

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

### `validate_stories.py` — Story Validator

Validates all story files in `docs/backlog/stories/` against the BMADder spec:

- **Frontmatter checks:** required fields, valid status/po_alignment/qa_status/agent_hint values
- **Consistency checks:** e.g., READY_FOR_DEV requires po_alignment=APPROVED
- **Section checks:** all 6 required sections present
- **Filename convention:** must match `story-NNNN-slug.md`

```bash
uv run scripts/validate_stories.py           # validate only
uv run scripts/validate_stories.py --fix     # auto-insert missing sections, then validate
```

### `preflight_auth.py` — Auth & Billing Safety

Pre-flight checks before running agent invocations:

1. **Billing safety** — Detects rogue env vars that silently switch to API billing
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
│   ├── bmadder.sh                ← Batch orchestrator (original)
│   ├── bmadder-iterative.sh      ← Iterative orchestrator (this doc)
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

## Design Decisions

### Why Story-at-a-Time Instead of Batch?

The original `bmadder.sh` batches all dev work, then all QA. This was a pragmatic choice — simpler control flow, easier to implement. But it deviates from the BMAD method's intention of handing work to the developer story by story with QA checking each one.

The iterative approach has real advantages:
- **Incremental deployability.** After every story, you have working code you could ship.
- **Faster feedback.** QA catches issues while the dev context is still warm (in git history).
- **Granular rollback.** Each QA PASS is a git commit. Roll back one story without losing all your work.
- **Better quota resilience.** If your agent quota runs out after 5 of 20 stories, you have 5 completed, tested, committed stories. With batch mode, you might have 20 half-developed stories and nothing deployable.

### Why Reset IN_DEV Instead of Resuming?

Each agent invocation starts with **fresh context** — no conversation history carries over. When a dev agent gets quota-killed mid-coding, the next agent has no idea what the previous agent was thinking. It would have to reverse-engineer a dead agent's half-finished thought process from dirty, potentially inconsistent files.

The stories are designed to be small and atomic. Redoing one from scratch takes the agent minutes, not hours. And any useful partial work the previous agent committed to git is still visible via `git log --oneline -20` (which the dev prompt explicitly instructs agents to check).

Incomplete code is categorically different from an incomplete story. A story with status "IN_DEV" is just a tag to update. Half-written source files with broken imports, orphaned function stubs, and partial test files can poison the entire codebase.

### Why Commit After PO Approval?

This creates an isolation boundary between planning artifacts (story files, epics) and code. If a dev agent corrupts the codebase, you can `git reset --hard` back to the PO checkpoint without losing any planning work. The approved story backlog remains intact for a fresh dev run.

### Why Not Reset PENDING_QA on Crash?

Unlike dev, the QA agent only **reads** code — it doesn't write source files. QA modifies only the story's frontmatter (setting `qa_status`) and appends notes to the `## QA Notes` section. If QA crashes, the code is untouched and already committed by the dev agent. Redoing QA is cheap — just re-read and re-check.

## Modifications Log

### 2026-03-15

#### `bmadder-iterative.sh` — Created
- **New iterative orchestrator following the BMAD method story-at-a-time lifecycle.**
  Each story goes through `DEV → QA → REFIX loop → COMPLETED` before the next story begins, producing incremental deployable versions.

- **PO approval checkpoint commit.**
  Commits all PO-approved story files before dev begins, creating an isolation boundary. Dev gets a clean baseline; if it fails, `git reset --hard` back to this commit preserves all planning work.

- **`--max-refix N` option (default: 3).**
  Per-story QA refix limit. Original batch mode only had a global max passes. The iterative mode applies the limit per story, so one stuck story doesn't consume all refix budget.

- **Crash recovery: IN_DEV → git clean + reset to READY_FOR_DEV.**
  When resuming from a crash, orphaned code from the dead agent is discarded via `git checkout -- . && git clean -fd`. The story is reset to READY_FOR_DEV for a clean dev restart. Reasoning: fresh context per call means a new agent can't resume a dead agent's thought process, and stories are small enough to redo cheaply. Orphaned code is more dangerous than wasted minutes.

- **Crash recovery: PENDING_QA → skip dev, redo QA.**
  If QA was interrupted, code is already committed by the dev agent. QA only reads code, so there's nothing to clean up. Just re-run QA.

- **Story queue priority on resume: PENDING_QA → REFIX → READY_FOR_DEV.**
  Interrupted work that's closest to completion processes first.

- **Per-story git commit + push on QA PASS.**
  Each completed story produces a deployable, rollback-friendly commit.

## License

MIT. See [LICENSE](LICENSE).
