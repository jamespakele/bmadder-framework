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

This creates the full folder structure, generates the orchestrator contract and standards files, initializes git, and checks your tooling.

### 4. Add your PRD and architecture

Fill in `docs/prd.md` and `docs/architecture.md`. These are the inputs that drive everything. Generate them with Perplexity, write them by hand, or use whatever works. They need to be specific enough that an agent can decompose them into implementable stories.

### 5. Run the auth preflight

```bash
uv run scripts/preflight_auth.py
```

Verifies your agent CLIs are installed, authenticated, and not accidentally billing to API keys instead of subscriptions.

### 6. Run the full cycle

```bash
./scripts/bmadder.sh cycle
```

This runs plan → dev → qa, with REFIX loops (max 3 passes). Go get coffee. Check `./scripts/bmadder.sh status` to see where things stand.

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

See `templates/story-template.md` for a blank story you can copy.

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
| `--timeout SECS` | 900 | Max seconds per agent invocation |
| `--story ID` | — | Target a specific story (e.g., `STORY-0001`) |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BMADDER_AGENT` | — | Force all phases to one agent |
| `BMADDER_MAX_ITER` | 10 | Max dev iterations per story |
| `BMADDER_STORY_TIMEOUT` | 900 | Max seconds per agent invocation |
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
| `scripts/bmadder.sh` | Main orchestrator — state machine, agent routing, phase execution |
| `scripts/bootstrap_bmadder.py` | One-command project setup |
| `scripts/init_bmadder.py` | Creates folder structure (called by bootstrap) |
| `scripts/create_rules.py` | Generates orchestrator contract and standards (called by bootstrap) |
| `scripts/validate_stories.py` | Validates story frontmatter against state machine |
| `scripts/preflight_auth.py` | Verifies agent auth and catches rogue API keys |

### Project Files (created by bootstrap)

| File | Purpose |
|------|---------|
| `.bmad/orchestrator-master.md` | Governing contract — state machine, roles, conventions |
| `.bmad/progress.txt` | Append-only dev progress log |
| `.bmad/logs/activity.log` | Structured activity log |
| `.bmad/.prompt-tmp.md` | Temp file for agent prompts (gitignored) |
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
│   ├── bmadder.sh                ← Main orchestrator
│   ├── bootstrap_bmadder.py      ← One-command setup
│   ├── init_bmadder.py           ← Folder structure creator
│   ├── create_rules.py           ← Rules/standards generator
│   ├── validate_stories.py       ← Frontmatter validator
│   └── preflight_auth.py         ← Auth/billing safety check
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
├── .bmad/
│   ├── orchestrator-master.md    ← Governing contract
│   ├── progress.txt              ← Append-only dev log
│   └── logs/
│       └── activity.log          ← Structured activity log
├── src/                          ← Your code goes here
│   └── scaffolding/              ← (optional) Stitch design artifacts
└── .mise.toml
```

## License

MIT. See [LICENSE](LICENSE).
