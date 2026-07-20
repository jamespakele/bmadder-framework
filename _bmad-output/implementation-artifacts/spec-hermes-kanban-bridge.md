---
title: 'Hermes Kanban Bridge: Python Observer Bridge (Phase 1)'
type: 'feature'
created: '2026-07-19'
status: 'in-review'
baseline_commit: '9778a3853c92a6c4e6074363915192fb3d3d2077'
context:
  - 'bmadder-core/src/story.rs'
  - 'bmadder-cli/src/story_io.rs'
  - 'bmadder-cli/src/logging.rs'
  - 'bmadder-cli/src/ui.rs'
  - 'bmadder-core/src/config.rs'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** BMADder produces rich story state (status transitions, QA results, activity logs) but has no way to surface that state in Hermes Kanban without manual updates.

**Approach:** A Python bridge script (`bmadder-kanban-bridge.py`) runs as a passive observer alongside BMADder. It polls story files and the activity log, calls `hermes kanban` CLI for card CRUD, and sends Telegram notifications on meaningful events. BMADder is never modified, never launched, and never touched.

## Boundaries & Constraints

**Always:**
- Bridge is read-only on `docs/backlog/stories/` and `_bmad/` (except its own state file)
- Card creation: `hermes kanban create --idempotency-key "bmadder:<board>:<story_id>"` (CLI, verified)
- Comments: `hermes kanban comment <task_id> <text> --author bmadder-bridge` (CLI, verified)
- Completion: `hermes kanban complete <task_id> --result "<r>" --metadata '<json>'` (CLI, verified)
- Status updates: REST API `PATCH /api/plugins/kanban/tasks/{task_id}` with `{"status": "<col>}` — verified as the only status setter; CLI has no `status`/`update` verb. REST is not SQLite — plan forbids only SQLite direct.
- Idempotency key `"bmadder:<board>:<story_id>"` prevents duplicate cards on restart
- Activity log tailing uses byte-offset seek — always resumes from last processed position
- Both pipe-format and bracket-format log lines must be parsed without crashing

**Ask First:**
- Changing the poll interval default (currently `--poll 10` seconds)
- Adding new Telegram event triggers beyond COMPLETED and ALL_DONE

**Never:**
- Modify any story file or `_bmad/` file (except `_bmad/kanban-bridge-state.json`)
- Run `git` — no commits, no pushes, no branch operations
- Launch or control `bmadder` — no subprocess spawning
- Write to Hermes SQLite DB directly — REST API and CLI only
- Use `delegate_task` or Hermes profiles to advance loop state
- Modify `ai-hi-lit/` — it is read-only

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|---|---|---|---|
| STORY_DISCOVER | `docs/backlog/stories/story-*.md` exists | Exactly one Kanban card created per file, idempotent key applied | Skip files missing required frontmatter fields; warn to stderr |
| STATUS_TRANSITION | Frontmatter `status` changes | Kanban card state updates to mapped state; comment added with log entry | Skip if card not found; warn stderr |
| ACTIVITY_LOG_TAIL | New lines in `_bmad/logs/activity.log` | Comment appended to matching card with actor, story_id, event, detail | Malformed lines skipped silently; warning to stderr |
| BRIDGE_RESTART | `_bmad/kanban-bridge-state.json` exists | Resume from byte offset; no duplicate cards; no duplicate comments | If state file corrupt, warn and reinitialize from zero |
| ALL_DONE | `progress.txt` contains `ALL_DONE` | Telegram completion message with story count | If Telegram fails, warn and continue |
| MISSING_STORY | Kanban update for unknown story_id | Skip silently; log warning | No crash; continue polling |

</frozen-after-approval>

## Code Map

- `bmadder-kanban-bridge.py` — primary implementation: story discovery, Hermes CLI calls, log tailing, state persistence, Telegram
- `requirements.txt` — Python dependencies (PyYAML, requests)
- `bmadder-core/src/story.rs` — `StoryStatus` enum (7 variants) and `StoryFrontmatter` struct; source of truth for status values and YAML fields
- `bmadder-cli/src/story_io.rs` — `parse_story_file()` reference; story file glob pattern `story-*.md`; `list_stories()` reference
- `bmadder-cli/src/logging.rs` — `log_activity()` for pipe-format details; `log_progress()` for progress.txt format; `log_marker()` for START/END markers
- `bmadder-cli/src/ui.rs` — `parse_activity_line()` for log parsing (pipe format `splitn(5, " | ")`); `stories_payload()` for card body format
- `bmadder-core/src/config.rs` — `PathsConfig` struct; paths: `stories_dir` = `docs/backlog/stories`, `state_dir` = `_bmad`

## Tasks & Acceptance

**Execution:**

- [x] `bmadder-kanban-bridge.py` -- `STATUS_MAP = {"DRAFT":"triage", "REVISE":"triage", "READY_FOR_DEV":"ready", "IN_DEV":"running", "PENDING_QA":"ready", "REFIX":"triage", "COMPLETED":"done"}` -- Map 7 `StoryStatus` variants to Hermes columns; keys use the serialized SCREAMING_SNAKE_CASE form (what the YAML frontmatter actually contains, per `story.rs:6` `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`); PENDING_QA→`ready` (not `review` — REST API rejects `review`); log warning for unknown statuses
- [x] `bmadder-kanban-bridge.py` -- `def create_card(board, story)` -- Call `hermes kanban create "<story_id>: <title>" --body "<card-body>" --idempotency-key "bmadder:<board>:<story_id>" --json`; when story status is `IN_DEV`, create with default `--initial-status running` (only way to land in `running`); when `DRAFT`/`REVISE`/`REFIX`, pass `--triage`; parse JSON stdout to capture `task_id`; never call SQLite directly
- [x] `bmadder-kanban-bridge.py` -- `def set_card_status(board, task_id, column)` -- `requests.patch("http://127.0.0.1:8000/api/plugins/kanban/tasks/<task_id>", params={"board": board}, json={"status": column})`; raise on 400/404; skip `running` (REST rejects it — log warning instead); treat `review` as `ready`
- [x] `bmadder-kanban-bridge.py` -- `def add_comment(board, card_id, text)` -- Call `hermes kanban comment <card_id> <text> --author bmadder-bridge`
- [x] `bmadder-kanban-bridge.py` -- `def complete_card(board, card_id, result, metadata)` -- Call `hermes kanban complete <card_id> --result "<result>" --metadata '<json>'`
- [x] `bmadder-kanban-bridge.py` -- `def block_card(board, card_id, kind, reason)` -- Call `hermes kanban block <card_id> --kind <kind> <reason>`
- [x] `bmadder-kanban-bridge.py` -- `def tail_activity_log(offset)` -- Read `_bmad/logs/activity.log` from byte `offset`; parse both pipe format `YYYY-MM-DDTHH:MM:SSZ | ACTOR | STORY_ID | EVENT | DETAIL` and bracket format `[TIMESTAMP] ACTOR STORY_ID EVENT: DETAIL`; return new lines + new byte offset; ignore malformed lines with warning to stderr
- [x] `bmadder-kanban-bridge.py` -- `def send_telegram(message)` -- POST to Telegram bot webhook via Hermes gateway; wrap in try/except; log warning and continue on failure
- [x] `bmadder-kanban-bridge.py` -- `--poll N` argument; main loop: discover story files → compute diff against known cards → create/update cards → tail activity log → append comments → sleep N seconds; persist state to `_bmad/kanban-bridge-state.json` after each pass
- [x] `bmadder-kanban-bridge.py` -- `def load_state(path)` and `def save_state(path, state)` -- JSON state file with full schema (see Design Notes); atomic write via temp file + rename; on corrupt JSON, warn and reinitialize
- [x] `bmadder-kanban-bridge.py` -- `def check_all_done()` -- Read `_bmad/logs/progress.txt`; if `ALL_DONE` found, trigger Telegram completion message
- [x] `bmadder-kanban-bridge.py` -- `def fingerprint(text)` -- SHA256 of comment text; store in state to prevent duplicate comments on restart
- [x] `requirements.txt` -- List `PyYAML>=0.6` and `requests>=2.28`; no other dependencies
- [x] `docs/backlog/stories/story-hermes-kanban-bridge.md` -- Story file for the bridge itself (type: feature, status: draft)

**Acceptance Criteria:**

- Given BMADder has story files in `docs/backlog/stories/story-*.md`, when the bridge starts with project path and board name, then exactly one Hermes Kanban task is created per story file using `--idempotency-key "bmadder:<board>:<story_id>"` and each task's state matches the frontmatter status per STATUS_MAP
- Given a story file's `status` field changes from `DRAFT` to `READY_FOR_DEV`, when the bridge polls, then the corresponding Kanban task's status updates to `ready` and a comment is added containing the latest activity log entry for that story
- Given the bridge was running and created tasks for all existing stories, when the bridge restarts, then no duplicate tasks are created (idempotency key prevents duplicates) and it resumes from the last recorded `activity_log_offset`
- Given BMADder writes new lines to `_bmad/logs/activity.log`, when the bridge polls, then only new lines (from byte offset) are parsed in both pipe and bracket formats and concise comments are added to the matching story task
- Given a story reaches `COMPLETED` with `qa_status: "PASS"`, when the bridge polls, then the Kanban task is completed with result `"QA PASS; bmadder committed and pushed"` and metadata `{"story_id":"...","qa_status":"PASS"}`
- Given a story transitions to `COMPLETED`, when the bridge processes the status change, then a Telegram message is sent with story ID, title, and commit hash; and when `ALL_DONE` appears in `progress.txt`, then a Telegram completion message is sent with total story count
- Given the bridge encounters a malformed line in activity log, when parsing, then it skips the line without crashing, logs a warning to stderr, and continues processing subsequent valid lines
- Given BMADder is running in the project directory, when the bridge is started or stopped, then no BMADder files are modified, no git operations occur, no BMADder processes are affected

## Spec Change Log

- 2026-07-19 — Initial spec from `docs/bmad-hermes-integration-plan.md`
- 2026-07-19 — Corrected status mapping per user (DRAFT/REVISE/REFIX→triage, IN_DEV→running, PENDING_QA→ready); verified Hermes CLI commands against `~/.hermes/hermes-agent/hermes_cli/kanban.py`; discovered plan's `hermes kanban status` verb does not exist — switched status updates to REST API `PATCH /api/plugins/kanban/tasks/{id}` (verified in `plugin_api.py:820`); `review` column is unreachable via REST (raises `unknown status: review`) so PENDING_QA remapped to `ready`; `running` cannot be set via PATCH (400) so IN_DEV lands only at card creation. Full state schema added per plan section 4.

## Design Notes

**Status mapping (verified against Hermes REST API + source `plugin_api.py:820`):**

| BMADder Status | Hermes Column | Reachable via |
|---|---|---|
| DRAFT | triage | `create --triage` or `PATCH {"status":"triage"}` |
| REVISE | triage | `PATCH {"status":"triage"}` |
| READY_FOR_DEV | ready | `PATCH {"status":"ready"}` or `hermes kanban promote` |
| IN_DEV | running | `create --initial-status running` (default) — `PATCH` rejects `running`; bridge sets this at card creation only |
| PENDING_QA | ready | `PATCH {"status":"ready"}` — **`review` is unreachable** via REST (handler raises `unknown status: review`); remapped to `ready` for the bridge |
| REFIX | triage | `PATCH {"status":"triage"}` |
| COMPLETED | done | `hermes kanban complete --result --metadata` |

**Verified Hermes CLI commands (source: `~/.hermes/hermes-agent/hermes_cli/kanban.py`):**

| Operation | Command | Source line |
|---|---|---|
| Create card | `hermes kanban create "<title>" --body "<body>" --idempotency-key "bmadder:<board>:<story_id>" --json` | kanban.py:307 |
| Add comment | `hermes kanban comment <task_id> <text>... --author bmadder-bridge` | kanban.py:517 |
| Complete task | `hermes kanban complete <task_id> --result "<r>" --metadata '<json>'` | kanban.py:543 |
| Block task | `hermes kanban block <task_id> <reason>` | kanban.py:575 |

**Status updates via REST API** (the only status setter — CLI has none):
```
PATCH http://127.0.0.1:8000/api/plugins/kanban/tasks/<task_id>?board=<board>
Content-Type: application/json
{"status": "triage"}  # one of: triage, todo, ready, done, archived, scheduled, blocked
```
Source: `plugins/kanban/dashboard/plugin_api.py:820` (`update_task`). Handler rejects `running` with 400; raises `unknown status: review`. So `review` is remapped to `ready` (PENDING_QA column).

**IN_DEV→running limitation:** REST API rejects direct `running` writes. Cards start `running` only at `hermes kanban create` (default `--initial-status running`) or via `hermes kanban claim`. The bridge creates cards with `running` only when the source story's status is `IN_DEV` at first discovery; for later transitions INTO `IN_DEV`, the bridge logs a warning (cannot mirror to `running` via PATCH) and adds a `comment` instead.

**Card body content** (from `bmadder-cli/src/ui.rs` `stories_payload()`):
```
**BMADder Story:** STORY-0023
**Source:** `docs/backlog/stories/story-0023-*.md`
- Status: `IN_DEV`
- PO alignment: `aligned`
- QA status: `null`
```

**Log format support (from `bmadder-cli/src/logging.rs` and `bmadder-cli/src/ui.rs`):**
- Pipe: `2026-07-16T19:00:00Z | ORCH | STORY-0023 | IN_DEV | dev via kimi-k2.7-code`
- Bracket: `[2026-07-16T19:00:00Z] ORCH STORY-0023 IN_DEV: dev via kimi-k2.7-code`
- `parse_activity_line()` uses `splitn(5, " | ")` for pipe format

**State file schema (`_bmad/kanban-bridge-state.json`, from plan section 4 + spam suppression):**
```json
{
  "version": 1,
  "board": "ai-r3",
  "last_poll": "2026-07-19T14:30:00Z",
  "activity_log_offset": 12345,
  "all_done_notified": false,
  "stories": {
    "STORY-0023": {
      "kanban_task_id": "task-uuid-here",
      "last_status": "IN_DEV",
      "last_qa_status": null,
      "last_comment_fingerprint": "sha256-of-last-comment"
    }
  }
}
```
`all_done_notified` suppresses duplicate "all done" Telegram messages across poll cycles (set true after first notify, reset when `ALL_DONE` leaves `progress.txt`). Persistence: atomic write (temp file + rename). Corruption: warn and reinitialize.

**Telegram message templates (from plan section 8):**
- Story completed: `✅ <story_id>: <title> — COMPLETED. QA: <qa_status>. Commit: <commit_hash>`
- All done: `🎉 All stories in <board> COMPLETED (<count> stories)`

## Verification

**Commands:**
- `python3 bmadder-kanban-bridge.py --help` -- expected: help text with all arguments
- `python3 -c "from bmadder_kanban_bridge import parse_story_file, STATUS_MAP; print('OK')"` -- expected: import success
- `python3 bmadder-kanban-bridge.py /path/to/project --board test --poll 10 --dry-run` -- expected: no errors, dry-run makes no CLI calls

**Manual checks (if no CLI):**
- Inspect `_bmad/kanban-bridge-state.json` after restart: `known_cards` should contain all discovered story IDs, `activity_log_offset` should be a positive integer
- Verify no files in `docs/backlog/stories/` or `_bmad/` are modified by the bridge (use `git status`)
- Verify `ai-hi-lit/` is never accessed or modified
