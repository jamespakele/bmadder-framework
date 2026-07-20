---
title: 'Hermes Bridge: JSONL Event Hooks in BMADder Rust (Phase 2)'
type: 'feature'
created: '2026-07-19'
status: 'in-review'
baseline_commit: '9778a3853c92a6c4e6074363915192fb3d3d2077'
context:
  - 'bmadder-core/src/story.rs'
  - 'bmadder-cli/src/logging.rs'
  - 'bmadder-cli/src/story_io.rs'
  - 'bmadder-cli/src/phases/iterative.rs'
  - 'bmadder-core/src/config.rs'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Phase 1 Python bridge uses regex to parse two activity log formats — brittle and dependent on log line structure. A format change breaks the parser silently.

**Approach:** Add structured JSONL event emission directly in BMADder Rust (`logging.rs`), gated behind a `--jsonl-events` flag. The Python bridge reads JSONL when present (higher priority than log parsing) and falls back to regex parsing when JSONL is unavailable. BMADder is never modified in behavior — it just optionally writes a second, machine-readable log alongside its human-readable one.

## Boundaries & Constraints

**Always:**
- `--jsonl-events` is disabled by default — no `events.jsonl` file created unless explicitly enabled
- JSONL entries are append-only, one event per line, never truncated
- Fallback to activity log parsing is automatic when `_bmad/logs/events.jsonl` is absent or malformed
- Event schema fields: `ts` (ISO 8601), `actor`, `story_id`, `event`, `from`, `to`, `detail`

**Ask First:**
- Changing event schema fields after Phase 1 ships
- Adding event types beyond status transitions

**Never:**
- Modify existing activity log format — JSONL is additive only
- Change the meaning of existing BMADder statuses
- Emit events for non-meaningful transitions (every-poll health checks, etc.)
- Block BMADder on JSONL write failures — log warning and continue

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|---|---|---|---|
| JSONL_ENABLED | `--jsonl-events` set; status transition occurs | One JSON object written to `_bmad/logs/events.jsonl` with all event fields | Write failure: warn to stderr, continue without blocking |
| JSONL_DISABLED | `--jsonl-events` not set | No `events.jsonl` created; bridge uses activity log parsing | N/A |
| JSONL_MALFORMED | `events.jsonl` contains invalid JSON lines | Bridge falls back to activity log parsing; malformed lines skipped with warning | Malformed lines counted in warning message |
| JSONL_ABSENT | `events.jsonl` does not exist | Bridge falls back to activity log parsing automatically | N/A |
| BRIDGE_READS_JSONL | JSONL events present + `--jsonl-events` flag | Bridge processes JSONL events with higher priority than activity log | If JSONL parse fails, degrade to activity log |

</frozen-after-approval>

## Code Map

- `bmadder-cli/src/logging.rs` — add `log_event()` function: appends structured JSON to `_bmad/logs/events.jsonl`; call it from every status-transition point
- `bmadder-cli/src/main.rs` — add `--jsonl-events` CLI flag; wire it into `Config`; pass to phases
- `bmadder-core/src/config.rs` — add `jsonl_events: bool` field to `Config` struct; wire into `from_args()` and TOML parsing
- `bmadder-cli/src/story_io.rs` — no changes needed (status updates flow through `update_story_status()` already)
- `bmadder-cli/src/phases/iterative.rs` — call `log_event()` in `process_one_story()`, `process_sm_po_loop()`, `process_dev_qa_loop()`, `check_all_done()`
- `bmadder-cli/src/moa.rs` — optionally emit `CONSENSUS` events when MOA applies a consensus result
- `bmadder-cli/src/git.rs` — emit `COMMIT` events with commit hash when `git commit` succeeds (if git.rs exists)

## Tasks & Acceptance

**Execution:**

- [x] `bmadder-core/src/config.rs` -- add `jsonl_events: bool` to `Config` struct; parse `--jsonl-events` CLI flag in `from_args()`; accept `jsonl_events = true` in TOML under `[defaults]`
- [x] `bmadder-cli/src/logging.rs` -- add `log_event(config: &Config, event: &StoryEvent) -> Result<(), Box<dyn Error>>`; open `_bmad/logs/events.jsonl` in append mode; write one JSON line per call; flush; define `StoryEvent` struct with fields: `ts: String`, `actor: String`, `story_id: String`, `event: String`, `from: Option<String>`, `to: Option<String>`, `detail: Option<String>`
- [x] `bmadder-cli/src/logging.rs` -- add `log_event()` call in `log_activity()` after writing the human-readable log line; guard with `if config.jsonl_events`
- [x] `bmadder-cli/src/phases/iterative.rs` -- add `log_event()` calls at: status transition points in `process_sm_po_loop()` (DRAFT→REVISE, DRAFT→READY_FOR_DEV, REVISE→READY_FOR_DEV), status transition points in `process_dev_qa_loop()` (IN_DEV→PENDING_QA, PENDING_QA→REFIX, REFIX→IN_DEV, PENDING_QA→COMPLETED), and `check_all_done()` (ALL_DONE marker)
- [x] `bmadder-cli/src/main.rs` -- add `--jsonl-events` flag to CLI parser; document it in `--help` output
- [x] `bmadder-cli/src/moa.rs` -- add `log_event()` call in `apply_po_consensus()` (APPROVE event) and `apply_qa_consensus()` (QA_PASS / QA_FAIL event); guard with `if config.jsonl_events`
- [x] `bmadder-cli/src/git.rs` -- add `log_event()` call after successful `git commit` with `event: "COMMIT"`, `detail: <commit_hash>`, `story_id: <target story>` (if this file exists and git operations are centralized there)
- [x] `bmadder-kanban-bridge.py` (Phase 1) -- add `def read_jsonl_events(path)` function: read all JSON lines from `_bmad/logs/events.jsonl`; return list of `StoryEvent` dicts; handle malformed lines gracefully (skip, warn)
- [x] `bmadder-kanban-bridge.py` (Phase 1) -- modify main loop: when `--jsonl-events` is enabled and `events.jsonl` exists, read and process JSONL events first; parse `from`/`to` fields for status mapping instead of inferring from frontmatter diff
- [x] `bmadder-kanban-bridge.py` (Phase 1) -- add `--jsonl-events` CLI flag; store in bridge config; pass to `read_jsonl_events()`; when JSONL is present, skip activity log parsing for already-processed story_ids

**Acceptance Criteria:**

- Given `--jsonl-events` is enabled, when a story status changes, then a structured JSON event is written to `_bmad/logs/events.jsonl` with fields: `ts`, `actor`, `story_id`, `event`, `from`, `to`, `detail`
- Given JSONL events are present in `_bmad/logs/events.jsonl`, when the bridge runs, then it processes JSONL events with higher priority than activity log parsing and no regex parsing is required for event extraction
- Given JSONL file is absent or malformed, when the bridge runs, then it falls back to activity log parsing without error
- Given `--jsonl-events` is disabled (default), when BMADder runs, then no `events.jsonl` file is created

## Spec Change Log

- 2026-07-19 — Initial spec from `docs/bmad-hermes-integration-plan.md`

## Design Notes

**Event schema:**

```json
{"ts":"2026-07-19T10:30:00Z","actor":"ORCH","story_id":"STORY-0023","event":"STATUS_CHANGE","from":"IN_DEV","to":"PENDING_QA","detail":"qa via kimi-k2.7-code"}
{"ts":"2026-07-19T10:35:00Z","actor":"QA","story_id":"STORY-0023","event":"QA_PASS","from":"PENDING_QA","to":"COMPLETED","detail":"all criteria met"}
{"ts":"2026-07-19T10:36:00Z","actor":"ORCH","story_id":"STORY-0023","event":"COMMIT","from":null,"to":null,"detail":"abc1234 (no verify)"}
```

**Bridge priority order:**
1. JSONL events (if `--jsonl-events` and file exists and non-empty)
2. Activity log parsing (pipe + bracket formats)
3. Frontmatter diff (fallback for status-only changes not yet in either log)

**JSONL write path is fire-and-forget**: if the write fails, warn to stderr but do not block BMADder's main loop.

## Verification

**Commands:**
- `cargo build --release` -- expected: clean build with no new warnings
- `cargo test` -- expected: all existing tests pass
- `cargo run -- --jsonl-events iterative --dry-run` on a project with stories -- expected: `_bmad/logs/events.jsonl` is created and contains valid JSON lines
- `cargo run -- iterative --dry-run` (no flag) -- expected: no `events.jsonl` created

**Manual checks (if no CLI):**
- Inspect `_bmad/logs/events.jsonl` for correct JSON format (one JSON object per line, no trailing comma, no array brackets)
- Verify activity log still contains the human-readable pipe-format lines unchanged
