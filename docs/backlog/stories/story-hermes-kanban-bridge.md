---
story_id: STORY-HERMES-BRIDGE
title: Hermes Kanban Bridge — Python Observer
status: DRAFT
priority: medium
agent_hint: specialist
created_at: 2026-07-19
updated_at: 2026-07-19
---

# Hermes Kanban Bridge

A passive Python bridge that mirrors BMADder story state into a Hermes Kanban board.

**BMADder produces state. Hermes observes and displays that state.**

## Source

- Script: `scripts/bmadder-kanban-bridge.py`
- Spec: `_bmad-output/implementation-artifacts/spec-hermes-kanban-bridge.md`
- Plan: `docs/bmad-hermes-integration-plan.md`

## Usage

```bash
python3 scripts/bmadder-kanban-bridge.py . --board <slug> --poll 10
```

## Acceptance

See `_bmad-output/implementation-artifacts/spec-hermes-kanban-bridge.md` → Tasks & Acceptance.