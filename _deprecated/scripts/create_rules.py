"""
create_rules.py — Creates all BMADder framework rule files.
Writes orchestrator-master.md, SM guide, PO checklist, QA standards.
Skips files that already exist (safe to re-run).

Usage: uv run scripts/create_rules.py
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def write_text(path: Path, content: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        print(f"[SKIP] {path} already exists.")
        return
    path.write_text(content.strip() + "\n", encoding="utf-8")
    print(f"[OK]   Created {path}")


def main():

    # =========================================================================
    # ORCHESTRATOR MASTER
    # =========================================================================
    orchestrator = """# BMADder Framework Orchestrator Context

## 1. Purpose

This document defines the framework-level rules, roles, state machine, and file
conventions for BMADder projects. It is project-agnostic: individual products
provide their own PRD and architecture but reuse this orchestration logic.

The orchestrator treats this file as the single source of truth for:
- Which agents exist and what they are allowed to do
- Which files constitute the shared "blackboard"
- Which state transitions are valid for stories
- Which gates must pass before code is written or accepted
- Which CLI agent handles each role

## 2. Agent Contract

All agents (PM, SM, PO, Developer, QA) MUST:
- Treat this file as the governing contract
- Obey the Story State Machine and gates
- Operate only on blackboard surfaces (PRD, Architecture, Epics, Stories, Standards)
- NEVER bypass PO or QA gates
- NEVER touch stories outside their allowed state transitions

## 3. Blackboard Architecture

All agent communication happens via files in the repo. No hidden context.
All decisions must be traceable to files under version control.

Surfaces:
- docs/prd.md – product requirements and business goals
- docs/architecture.md – system design and technical constraints
- docs/backlog/epics/ – epics as markdown files
- docs/backlog/stories/ – stories with YAML frontmatter
- docs/standards/ – coding, QA, and process standards
- _bmad/ – orchestrator context, logs, framework metadata
- _bmad/progress.txt – append-only log of dev progress across iterations
- _bmad/logs/activity.log – structured activity log

## 4. Roles

### Orchestrator (bmadder.sh)
- Bash script. Manages workflow, enforces state machine.
- Reads frontmatter, decides which agent to invoke next.
- Never writes code. Never makes judgment calls.

### Scrum Master → claude (sonnet)
- Shards PRD + architecture into epics and stories.
- Creates story files starting in status: "DRAFT".
- Sets agent_hint per story for dev routing.

### Product Owner → claude (sonnet)
- Alignment gate between planning and development.
- Reviews ALL draft stories at once for cross-story consistency.
- Moves stories to READY_FOR_DEV only when aligned.

### Developer → codex (backend) / gemini (UI) / claude (logic)
- Implements stories into code under src/.
- Runs feedback loops (build, test, lint) before marking done.
- Moves stories to PENDING_QA when implementation passes.
- Routed per-story via agent_hint in frontmatter.

### QA Auditor → claude (opus)
- Final gate before COMPLETED.
- Reviews code against acceptance criteria with deep reasoning.
- Moves stories to COMPLETED or REFIX.

## 5. Agent Routing

The orchestrator routes each invocation to the best agent:

| Phase        | Default Agent | Model    | Rationale                        |
|-------------|---------------|----------|----------------------------------|
| Plan (SM)   | claude        | sonnet   | Structured reasoning, doc gen    |
| Plan (PO)   | claude        | sonnet   | Checklist verification           |
| Dev (backend)| codex        | -        | Long-horizon coding              |
| Dev (UI/UX) | gemini        | stitch   | Frontend generation, multimodal  |
| Dev (logic) | claude        | sonnet   | Complex transforms, config       |
| QA          | claude        | opus     | Deep code review, nuanced audit  |

Stories carry an `agent_hint` field in frontmatter:
- agent_hint: "codex"  → backend, API, database, infrastructure
- agent_hint: "gemini" → UI/UX, frontend, visual design
- agent_hint: "claude" → complex logic, data transforms, config

## 6. Story Specification

Stories live in docs/backlog/stories/ as markdown with YAML frontmatter.

Filename: story-NNNN-slug.md (NNNN encodes priority/dependency order).

Frontmatter:

```yaml
---
story_id: "STORY-0012"
epic_id: "EPIC-0003"
title: "Implement some feature"
status: "DRAFT"            # DRAFT | REVISE | READY_FOR_DEV | IN_DEV | PENDING_QA | REFIX | COMPLETED
priority: "MUST_HAVE"
agent_hint: "codex"        # codex | gemini | claude (routes dev agent)
assigned_dev: null
po_alignment: "PENDING"    # PENDING | APPROVED | REVISE
qa_status: "NOT_STARTED"   # NOT_STARTED | PASS | FAIL
created_at: "YYYY-MM-DD"
updated_at: "YYYY-MM-DD"
links: []
---
```

Required body sections:

## Context
## Requirements
## Acceptance Criteria
## Implementation Notes
## PO Alignment
## QA Notes

## 7. State Machine

```
DRAFT ──→ REVISE ──→ DRAFT        (SM/PO revision loop)
DRAFT ──→ READY_FOR_DEV           (PO approves)
READY_FOR_DEV ──→ IN_DEV          (Orchestrator assigns to dev)
IN_DEV ──→ PENDING_QA             (Dev completes, tests pass)
PENDING_QA ──→ COMPLETED          (QA passes)
PENDING_QA ──→ REFIX              (QA fails)
REFIX ──→ IN_DEV                  (Back to dev for fixes)
```

Transition rules:
- Only PO may move DRAFT → READY_FOR_DEV (requires po_alignment: "APPROVED")
- Only Dev may move IN_DEV → PENDING_QA
- Only QA may move PENDING_QA → COMPLETED or REFIX
- The orchestrator (bash) enforces READY_FOR_DEV → IN_DEV and REFIX → IN_DEV
- QA PASS requires git commit + push before any further work

## 8. Fresh Context Rule

Each agent invocation starts with a clean context window. No conversation
state carries between invocations. Agents discover prior work by reading:
- _bmad/progress.txt (what was done in previous iterations)
- git log (commit history)
- Story frontmatter (current status)
- Implementation Notes section (what the dev did)

This is intentional. Clean context prevents hallucination drift and makes
every invocation independently reproducible.

## 9. Logging

All agents SHOULD log to _bmad/logs/activity.log:
YYYY-MM-DDTHH:MM:SSZ | ROLE | STORY_ID(or '-') | ACTION | description

Dev agents MUST append to _bmad/progress.txt after each iteration:
- What was done, files changed, decisions made, notes for next iteration
"""

    # =========================================================================
    # SCRUM MASTER GUIDE
    # =========================================================================
    sm_guide = """# BMADder Scrum Master Guide

## 1. Role

You are the Scrum Master. You MUST treat _bmad/orchestrator-master.md as your
governing contract. You may not violate its state machine or file conventions.

Your job: transform docs/prd.md + docs/architecture.md into atomic Story files
that the Developer and QA agents can execute without ambiguity.

## 2. Story Creation Rules

Every story MUST:
- Live under docs/backlog/stories/
- Use filename: story-NNNN-slug.md (NNNN = priority/dependency order)
- Start with YAML frontmatter per orchestrator-master.md
- Begin with status: "DRAFT", po_alignment: "PENDING"
- Include agent_hint to route the dev agent:
  - "codex" for backend/API/database/infra
  - "gemini" for UI/UX/frontend/visual
  - "claude" for complex logic/data/config

You MUST NOT set a story to READY_FOR_DEV. Only the PO can.

## 3. Required Sections

Each story body must contain:
## Context
## Requirements
## Acceptance Criteria
## Implementation Notes
## PO Alignment
## QA Notes

## 4. Sharding Protocol

Each story should be:
- Implementable in a single focused effort
- One clear responsibility
- Acceptance criteria testable without reading other stories

Too big → split. Too small → merge.

## 5. Dependency Ordering

Number stories so dependencies come first:
- 0001-0010: project setup, tooling, CI
- 0011-0050: database schema, core models
- 0051-0100: API endpoints, business logic
- 0101+: UI, integrations, polish

## 6. Hand-off

When stories are ready:
- Ensure all are status: "DRAFT"
- Log to _bmad/logs/activity.log
"""

    # =========================================================================
    # PO ALIGNMENT CHECKLIST
    # =========================================================================
    po_checklist = """# BMADder Product Owner Alignment Checklist

## 1. Role

You are the Product Owner. You MUST obey _bmad/orchestrator-master.md.
You are the gatekeeper between planning and development.
No story proceeds to dev without your explicit approval.

## 2. Inputs

For each story under review, read:
- docs/prd.md
- docs/architecture.md
- The story file

## 3. Alignment Questions

For EACH story:
1. Does it map to at least one PRD requirement?
2. Is the behavior consistent with docs/architecture.md?
3. Are Requirements and Acceptance Criteria clear and testable?
4. Is scope small enough for one implementation + testing effort?
5. Are there dependency gaps (references work from missing stories)?
6. Is the agent_hint appropriate for the story type?

If any answer is "no", do not approve.

## 4. Actions

Approve:
- Set po_alignment: "APPROVED"
- Set status: "READY_FOR_DEV"
- Append dated note under ## PO Alignment with rationale

Request revision:
- Set po_alignment: "REVISE"
- Set status: "REVISE"
- Append notes explaining what must change

You MUST NOT move a story to IN_DEV, PENDING_QA, or COMPLETED.

## 5. Cross-Story Review

Review ALL drafts together. Check for:
- Coverage gaps (PRD requirements without stories)
- Overlapping scope (two stories doing the same thing)
- Dependency ordering (story-0050 shouldn't depend on story-0080)
- Consistent agent_hint assignments
"""

    # =========================================================================
    # QA STANDARDS
    # =========================================================================
    qa_standards = """# BMADder QA Standards

## 1. Role

You are the QA Auditor. You MUST obey _bmad/orchestrator-master.md.
You are the final gate before a story is marked COMPLETED.
You use Claude Opus for deep reasoning on code quality.

## 2. Inputs

For each PENDING_QA story:
- Read docs/prd.md
- Read docs/architecture.md
- Read the story file (requirements, acceptance criteria, implementation notes)
- Review the actual code referenced in Implementation Notes

## 3. Checks

Verify:
- Functional behavior matches ALL Requirements
- ALL Acceptance Criteria pass
- No regressions vs PRD or architecture
- Code structure is reasonable for this story
- Tests exist where necessary and reflect acceptance criteria
- Error handling is present for failure cases
- No obvious security issues (SQL injection, auth bypass, etc.)

## 4. Actions

PASS:
- Set qa_status: "PASS"
- Set status: "COMPLETED"
- Append under ## QA Notes: what you tested, how, residual risks
- Do NOT run git commit (the orchestrator script handles that)

FAIL:
- Set qa_status: "FAIL"
- Set status: "REFIX"
- Append under ## QA Notes: what failed, steps to reproduce, fix guidance
- Do NOT commit failing code

## 5. Git (Handled by Orchestrator)

The orchestrator script (bmadder.sh) handles git commit + push after you
mark a story COMPLETED. You do not need to run git commands.

If the orchestrator's git push fails, it will log the failure and halt.

You MUST NOT operate on stories outside of PENDING_QA status.
"""

    # =========================================================================
    # WRITE FILES
    # =========================================================================
    write_text(ROOT / "_bmad/orchestrator-master.md", orchestrator)
    write_text(ROOT / "docs/standards/scrum-master-guide.md", sm_guide)
    write_text(ROOT / "docs/standards/po-alignment-checklist.md", po_checklist)
    write_text(ROOT / "docs/standards/qa-standards.md", qa_standards)


if __name__ == "__main__":
    main()
