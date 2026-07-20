#!/usr/bin/env python3
"""BMADder -> Hermes Kanban Bridge.

A passive observer that mirrors BMADder story state into a Hermes Kanban board.
BMADder produces state; Hermes observes and displays that state. The bridge never
modifies BMADder files, never runs git, and never launches bmadder.

Usage:
    python3 bmadder-kanban-bridge.py <project_path> --board <slug> [--poll 10] [--dry-run]

Verified against:
    - bmadder-core/src/story.rs        (StoryStatus enum, StoryFrontmatter)
    - bmadder-cli/src/logging.rs       (activity.log pipe format)
    - bmadder-cli/src/ui.rs            (parse_activity_line, stories_payload)
    - ~/.hermes/hermes-agent/hermes_cli/kanban.py  (CLI surface)
    - ~/.hermes/hermes-agent/plugins/kanban/dashboard/plugin_api.py:820  (REST PATCH)
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

import yaml

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

# Map BMADder StoryStatus (serialized SCREAMING_SNAKE_CASE per story.rs:6) to
# Hermes Kanban columns. PENDING_QA -> "ready" (not "review") because the Hermes
# REST PATCH handler raises "unknown status: review" (plugin_api.py:873).
# IN_DEV -> "running" is only settable at card creation (--initial-status running);
# the REST PATCH rejects "running" with 400 (plugin_api.py:866-869).
STATUS_MAP: dict[str, str] = {
    "DRAFT": "triage",
    "REVISE": "triage",
    "READY_FOR_DEV": "ready",
    "IN_DEV": "running",
    "PENDING_QA": "ready",
    "REFIX": "triage",
    "COMPLETED": "done",
}

# Statuses the REST PATCH endpoint accepts as direct writes.
# "running" is NOT in this set (rejected). "review" is NOT in this set (unknown).
PATCHABLE_STATUSES = {"triage", "todo", "ready", "done", "archived", "scheduled", "blocked"}

# Idempotency key format: bmadder:<board>:<story_id>
IDEMPOTENCY_KEY_FMT = "bmadder:{board}:{story_id}"

# State file schema version.
STATE_VERSION = 1

# REST API base for the Hermes Kanban plugin (gateway default port 8000).
# Overridden in main() from [hermes].rest_url in bmadder.toml.
REST_BASE = os.environ.get("HERMES_KANBAN_REST", "http://127.0.0.1:8000")
REST_TASK_PATH = "/api/plugins/kanban/tasks/{task_id}"

# Hermes CLI binary — overridden in main() from [hermes].hermes_home.
# Resolved to <hermes_home>/hermes-agent/venv/bin/hermes, falling back to
# `hermes` on PATH.
HERMES_BINARY = "hermes"

# Activity log line formats.
# Pipe:   2026-07-16T19:00:00Z | ORCH | STORY-0023 | IN_DEV | dev via kimi-k2.7-code
# Bracket: [2026-07-16T19:00:00Z] ORCH STORY-0023 IN_DEV: dev via kimi-k2.7-code
PIPE_RE = re.compile(r"^(?P<ts>\S+)\s*\|\s*(?P<actor>[^|]+?)\s*\|\s*(?P<story_id>[^|]+?)\s*\|\s*(?P<event>[^|]+?)\s*\|\s*(?P<detail>.+)$")
BRACKET_RE = re.compile(r"^\[(?P<ts>[^\]]+)\]\s+(?P<actor>\S+)\s+(?P<story_id>\S+)\s+(?P<event>[^:]+):\s*(?P<detail>.+)$")


# ---------------------------------------------------------------------------
HERMES_REST_DEFAULT = "http://127.0.0.1:8000"


@dataclass
class HermesConfig:
    """Hermes bridge config read from bmadder.toml [hermes] section."""

    bridge_report: bool = False
    project_slug: str = ""
    hermes_home: str = "~/.hermes"
    rest_url: str = ""
    bridge_script: str = ""
    bridge_poll_seconds: int = 10

    @property
    def rest_base(self) -> str:
        url = self.rest_url.strip()
        return url.rstrip("/") if url else HERMES_REST_DEFAULT

    def board_slug(self, project_root: Path) -> str:
        if self.project_slug:
            return self.project_slug
        return project_root.name.lower().replace("_", "-")

    @property
    def hermes_binary(self) -> str:
        """Resolve the `hermes` CLI binary from hermes_home, fallback to PATH."""
        home = os.path.expanduser(self.hermes_home)
        candidate = os.path.join(home, "hermes-agent", "venv", "bin", "hermes")
        if os.path.exists(candidate):
            return candidate
        return "hermes"


def load_hermes_config(toml_path: Path) -> HermesConfig:
    """Read the [hermes] section from bmadder.toml. Returns defaults if missing.

    Uses tomllib (Python 3.11+ stdlib) for parsing. Falls back to a minimal
    line-based parser if tomllib is unavailable.
    """
    if not toml_path.exists():
        return HermesConfig()
    try:
        text = toml_path.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"warn: cannot read {toml_path}: {exc}", file=sys.stderr)
        return HermesConfig()
    # Prefer stdlib tomllib (3.11+).
    try:
        import tomllib
        data = tomllib.loads(text)
        hermes = data.get("hermes") or {}
        return HermesConfig(
            bridge_report=bool(hermes.get("bridge_report", False)),
            project_slug=str(hermes.get("project_slug", "")).strip(),
            hermes_home=str(hermes.get("hermes_home", "~/.hermes")).strip() or "~/.hermes",
            rest_url=str(hermes.get("rest_url", "")).strip(),
            bridge_script=str(hermes.get("bridge_script", "")).strip(),
            bridge_poll_seconds=int(hermes.get("bridge_poll_seconds", 10)),
        )
    except ImportError:
        return _parse_hermes_section_fallback(text)


def _parse_hermes_section_fallback(text: str) -> HermesConfig:
    """Minimal line-based parser for the [hermes] section (no TOML lib)."""
    cfg = HermesConfig()
    in_hermes = False
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            in_hermes = (line == "[hermes]")
            continue
        if not in_hermes or "=" not in line:
            continue
        key, _, val = line.partition("=")
        key = key.strip()
        val = val.strip().strip('"').strip("'")
        if key == "bridge_report":
            cfg.bridge_report = val.lower() in ("true", "1", "yes")
        elif key == "project_slug":
            cfg.project_slug = val
        elif key == "hermes_home":
            cfg.hermes_home = val or "~/.hermes"
        elif key == "rest_url":
            cfg.rest_url = val
        elif key == "bridge_script":
            cfg.bridge_script = val
        elif key == "bridge_poll_seconds":
            try:
                cfg.bridge_poll_seconds = int(val)
            except ValueError:
                pass
    return cfg


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class Story:
    """A parsed BMADder story file."""

    path: Path
    story_id: str
    title: str
    status: str
    qa_status: Optional[str] = None
    po_alignment: Optional[str] = None
    body: str = ""


@dataclass
class LogEntry:
    """One parsed activity.log line."""

    ts: str
    actor: str
    story_id: str
    event: str
    detail: str

    def to_comment(self) -> str:
        return f"{self.actor} {self.story_id} {self.event} · {self.detail}"


@dataclass
class StoryState:
    """Per-story tracking held in the state file."""

    kanban_task_id: str
    last_status: Optional[str] = None
    last_qa_status: Optional[str] = None
    last_comment_fingerprint: Optional[str] = None


@dataclass
class BridgeState:
    """Checkpoint state for idempotent restart."""

    version: int = STATE_VERSION
    board: str = ""
    last_poll: Optional[str] = None
    activity_log_offset: int = 0
    all_done_notified: bool = False
    stories: dict[str, StoryState] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "version": self.version,
            "board": self.board,
            "last_poll": self.last_poll,
            "activity_log_offset": self.activity_log_offset,
            "all_done_notified": self.all_done_notified,
            "stories": {
                sid: {
                    "kanban_task_id": s.kanban_task_id,
                    "last_status": s.last_status,
                    "last_qa_status": s.last_qa_status,
                    "last_comment_fingerprint": s.last_comment_fingerprint,
                }
                for sid, s in self.stories.items()
            },
        }

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "BridgeState":
        stories: dict[str, StoryState] = {}
        for sid, s in (d.get("stories") or {}).items():
            stories[sid] = StoryState(
                kanban_task_id=s.get("kanban_task_id", ""),
                last_status=s.get("last_status"),
                last_qa_status=s.get("last_qa_status"),
                last_comment_fingerprint=s.get("last_comment_fingerprint"),
            )
        return cls(
            version=d.get("version", STATE_VERSION),
            board=d.get("board", ""),
            last_poll=d.get("last_poll"),
            activity_log_offset=d.get("activity_log_offset", 0),
            all_done_notified=d.get("all_done_notified", False),
            stories=stories,
        )


# ---------------------------------------------------------------------------
# Story parsing
# ---------------------------------------------------------------------------


def parse_story_file(path: Path) -> Optional[Story]:
    """Parse a BMADder story file: YAML frontmatter between `---` fences + body.

    Returns None (with a stderr warning) if the file is missing required fields.
    Mirrors bmadder-cli/src/story_io.rs::parse_story_file.
    """
    try:
        content = path.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"warn: cannot read {path}: {exc}", file=sys.stderr)
        return None

    lines = content.splitlines()
    if not lines or lines[0].strip() != "---":
        print(f"warn: {path}: missing opening frontmatter fence", file=sys.stderr)
        return None

    yaml_lines: list[str] = []
    body_start = 1
    for i, line in enumerate(lines[1:], start=1):
        if line.strip() == "---":
            body_start = i + 1
            break
        yaml_lines.append(line)
    else:
        print(f"warn: {path}: missing closing frontmatter fence", file=sys.stderr)
        return None

    try:
        fm = yaml.safe_load("\n".join(yaml_lines)) or {}
    except yaml.YAMLError as exc:
        print(f"warn: {path}: YAML parse error: {exc}", file=sys.stderr)
        return None

    if not isinstance(fm, dict):
        print(f"warn: {path}: frontmatter is not a mapping", file=sys.stderr)
        return None

    story_id = str(fm.get("story_id", "")).strip()
    title = str(fm.get("title", "")).strip()
    status = str(fm.get("status", "")).strip()

    # Derive story_id from filename if missing (mirrors story_io.rs:48-63).
    if not story_id:
        stem = path.stem
        digits = next((p for p in stem.split("-") if p.isdigit()), "")
        if digits:
            story_id = f"STORY-{digits}"
        else:
            story_id = stem.upper().replace("-", "_")

    if not title or not status:
        print(f"warn: {path}: missing title or status", file=sys.stderr)
        return None

    if status not in STATUS_MAP:
        print(f"warn: {path}: unknown status {status!r}", file=sys.stderr)

    body = "\n".join(lines[body_start:])
    return Story(
        path=path,
        story_id=story_id,
        title=title,
        status=status,
        qa_status=(str(fm["qa_status"]).strip() if fm.get("qa_status") else None),
        po_alignment=(str(fm["po_alignment"]).strip() if fm.get("po_alignment") else None),
        body=body,
    )


def list_stories(stories_dir: Path) -> list[Story]:
    """Discover all story-*.md files and parse them. Mirrors story_io.rs::list_stories."""
    if not stories_dir.is_dir():
        return []
    stories: list[Story] = []
    for p in sorted(stories_dir.glob("story-*.md")):
        s = parse_story_file(p)
        if s is not None:
            stories.append(s)
    return stories


def card_body(story: Story, project_root: Path) -> str:
    """Build the Kanban card body. References bmadder-cli/src/ui.rs stories_payload."""
    rel = story.path.relative_to(project_root) if story.path.is_absolute() else story.path
    lines = [
        f"**BMADder Story:** {story.story_id}",
        f"**Source:** `{rel}`",
        "",
        f"- Status: `{story.status}`",
        f"- PO alignment: `{story.po_alignment or 'null'}`",
        f"- QA status: `{story.qa_status or 'null'}`",
    ]
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Activity log tailing
# ---------------------------------------------------------------------------


def parse_activity_line(line: str) -> Optional[LogEntry]:
    """Parse one activity.log line in pipe or bracket format.

    Mirrors bmadder-cli/src/ui.rs::parse_activity_line (pipe format) and adds
    bracket-format support. Returns None for malformed lines.
    """
    line = line.rstrip("\n")
    if not line.strip():
        return None
    m = PIPE_RE.match(line)
    if not m:
        m = BRACKET_RE.match(line)
    if not m:
        return None
    return LogEntry(
        ts=m.group("ts").strip(),
        actor=m.group("actor").strip(),
        story_id=m.group("story_id").strip(),
        event=m.group("event").strip(),
        detail=m.group("detail").strip(),
    )


def tail_activity_log(log_path: Path, offset: int) -> tuple[list[LogEntry], int]:
    """Read new lines from log_path starting at byte offset.

    Returns (parsed_entries, new_byte_offset). Malformed lines are skipped with a
    stderr warning (AC-7). If the file shrank or was truncated, reset to 0.
    """
    if not log_path.exists():
        return [], 0
    size = log_path.stat().st_size
    if size < offset:
        # File was truncated/rotated — start from beginning.
        print(f"warn: {log_path} shrank ({size} < {offset}); resetting offset to 0", file=sys.stderr)
        offset = 0
    if size == offset:
        return [], offset
    with log_path.open("rb") as f:
        f.seek(offset)
        chunk = f.read(size - offset)
    new_offset = size
    entries: list[LogEntry] = []
    for raw in chunk.decode("utf-8", errors="replace").splitlines():
        e = parse_activity_line(raw)
        if e is None:
            if raw.strip():
                print(f"warn: malformed activity log line skipped: {raw[:120]!r}", file=sys.stderr)
            continue
        entries.append(e)
    return entries, new_offset


def read_jsonl_events(jsonl_path: Path) -> list[dict[str, Any]]:
    """Read all JSONL events from BMADder's structured event log.

    Returns a list of event dicts (fields: ts, actor, story_id, event, from, to, detail).
    Malformed lines are skipped with a stderr warning (graceful degradation).
    Empty list if the file is absent.
    """
    if not jsonl_path.exists():
        return []
    events: list[dict[str, Any]] = []
    try:
        text = jsonl_path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        print(f"warn: cannot read {jsonl_path}: {exc}", file=sys.stderr)
        return []
    for i, raw in enumerate(text.splitlines(), start=1):
        raw = raw.strip()
        if not raw:
            continue
        try:
            ev = json.loads(raw)
        except json.JSONDecodeError as exc:
            print(f"warn: {jsonl_path}:{i}: malformed JSON skipped: {exc}", file=sys.stderr)
            continue
        if not isinstance(ev, dict):
            print(f"warn: {jsonl_path}:{i}: non-object JSON line skipped", file=sys.stderr)
            continue
        events.append(ev)
    return events


# ---------------------------------------------------------------------------
# Hermes CLI wrappers
# ---------------------------------------------------------------------------


def _run_cli(args: list[str], dry_run: bool = False) -> Optional[str]:
    """Run a hermes CLI command and return stdout (str). Returns None on failure."""
    if dry_run:
        print(f"[dry-run] would run: {' '.join(args)}", file=sys.stderr)
        return None
    try:
        proc = subprocess.run(args, capture_output=True, text=True, check=False)
    except FileNotFoundError:
        print(f"warn: hermes CLI not found on PATH for {args[0]!r}", file=sys.stderr)
        return None
    if proc.returncode != 0:
        print(f"warn: {' '.join(args)} failed (exit {proc.returncode}): {proc.stderr.strip()}", file=sys.stderr)
        return None
    return proc.stdout


def hermes_kanban_create(
    board: str, story: Story, project_root: Path, dry_run: bool = False
) -> Optional[str]:
    """Create a Kanban card for the story. Returns the task_id, or None on failure.

    Uses --idempotency-key so restarts never duplicate. Initial status is chosen
    from the story's current status: IN_DEV -> running (default), DRAFT/REVISE/REFIX
    -> triage (--triage flag), others rely on a follow-up PATCH.
    """
    title = f"{story.story_id}: {story.title}"
    idem = IDEMPOTENCY_KEY_FMT.format(board=board, story_id=story.story_id)
    args = [
        HERMES_BINARY, "kanban", "create", title,
        "--body", card_body(story, project_root),
        "--idempotency-key", idem,
        "--board", board,
        "--json",
    ]
    # Landing column at creation time.
    col = STATUS_MAP.get(story.status, "triage")
    if story.status in ("DRAFT", "REVISE", "REFIX"):
        args.append("--triage")
    elif story.status == "IN_DEV":
        # Default --initial-status is "running"; leave it. This is the ONLY way
        # to land a card in `running` (REST PATCH rejects it).
        pass
    else:
        # For READY_FOR_DEV/PENDING_QA/COMPLETED, create with --triage then PATCH.
        args.append("--triage")
    out = _run_cli(args, dry_run=dry_run)
    if out is None:
        return None
    try:
        data = json.loads(out)
        return data.get("id")
    except json.JSONDecodeError:
        # Non-JSON fallback: parse "Created <id>" line.
        m = re.search(r"Created\s+(\S+)", out)
        return m.group(1) if m else None


def hermes_kanban_comment(task_id: str, text: str, board: str, dry_run: bool = False) -> bool:
    """Append a comment to a Kanban task."""
    args = [HERMES_BINARY, "kanban", "comment", task_id, text, "--author", "bmadder-bridge", "--board", board]
    return _run_cli(args, dry_run=dry_run) is not None


def hermes_kanban_complete(
    task_id: str, result: str, metadata: dict[str, Any], board: str, dry_run: bool = False
) -> bool:
    """Mark a Kanban task done with a result and structured metadata."""
    args = [
        HERMES_BINARY, "kanban", "complete", task_id,
        "--result", result,
        "--metadata", json.dumps(metadata),
        "--board", board,
    ]
    return _run_cli(args, dry_run=dry_run) is not None


def hermes_kanban_block(
    task_id: str, kind: str, reason: str, board: str, dry_run: bool = False
) -> bool:
    """Block a Kanban task with a typed reason."""
    args = [HERMES_BINARY, "kanban", "block", task_id, "--kind", kind, reason, "--board", board]
    return _run_cli(args, dry_run=dry_run) is not None


# ---------------------------------------------------------------------------
# REST API status setter
# ---------------------------------------------------------------------------


def set_card_status(task_id: str, column: str, board: str, dry_run: bool = False) -> bool:
    """Set a card's status column via the Hermes REST API.

    The CLI has no status setter; the only setter is PATCH /api/plugins/kanban/tasks/{id}
    (plugin_api.py:820). "running" is rejected (400) and "review" is unknown (400);
    this function remaps and warns accordingly.
    """
    if column == "running":
        print(f"warn: cannot PATCH status to 'running' (rejected by API) for task {task_id}; adding comment instead", file=sys.stderr)
        return False
    if column == "review":
        print(f"warn: 'review' is not a settable status (unknown to PATCH handler); remapping to 'ready' for task {task_id}", file=sys.stderr)
        column = "ready"
    if column not in PATCHABLE_STATUSES:
        print(f"warn: status {column!r} is not directly settable; skipping for task {task_id}", file=sys.stderr)
        return False
    if column == "done":
        # "done" via PATCH calls complete_task; prefer the CLI for result/metadata.
        print(f"warn: use hermes_kanban_complete for 'done' (carries result+metadata); skipping PATCH for task {task_id}", file=sys.stderr)
        return False

    url = REST_BASE + REST_TASK_PATH.format(task_id=task_id)
    payload = {"status": column}
    if dry_run:
        print(f"[dry-run] would PATCH {url} board={board} {payload}", file=sys.stderr)
        return True
    try:
        import requests
    except ImportError:
        print("warn: 'requests' not installed; cannot PATCH status", file=sys.stderr)
        return False
    try:
        r = requests.patch(url, params={"board": board}, json=payload, timeout=10)
    except Exception as exc:
        print(f"warn: PATCH {url} failed: {exc}", file=sys.stderr)
        return False
    if r.status_code >= 400:
        print(f"warn: PATCH {url} -> {r.status_code}: {r.text[:200]}", file=sys.stderr)
        return False
    return True


# ---------------------------------------------------------------------------
# State file persistence
# ---------------------------------------------------------------------------


def state_path_for(project_root: Path) -> Path:
    return project_root / "_bmad" / "kanban-bridge-state.json"


def load_state(path: Path, board: str) -> BridgeState:
    """Load bridge state. On corruption or missing file, reinitialize cleanly."""
    if not path.exists():
        return BridgeState(board=board)
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        st = BridgeState.from_dict(data)
        if not st.board:
            st.board = board
        return st
    except (OSError, json.JSONDecodeError) as exc:
        print(f"warn: state file {path} corrupt ({exc}); reinitializing", file=sys.stderr)
        return BridgeState(board=board)


def save_state(path: Path, state: BridgeState) -> None:
    """Atomically write state (temp file + rename) to prevent corruption."""
    path.parent.mkdir(parents=True, exist_ok=True)
    blob = json.dumps(state.to_dict(), indent=2)
    fd, tmp = tempfile.mkstemp(prefix=".kanban-bridge-state-", dir=str(path.parent))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(blob)
        os.replace(tmp, path)
    except Exception:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def fingerprint(text: str) -> str:
    """SHA256 of comment text — used to dedup comments on restart (AC-3/AC-4)."""
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


# ---------------------------------------------------------------------------
# Telegram
# ---------------------------------------------------------------------------


def send_telegram(message: str, dry_run: bool = False) -> None:
    """Send a Telegram message via the Hermes gateway. Best-effort: never blocks."""
    if dry_run:
        print(f"[dry-run] telegram: {message}", file=sys.stderr)
        return
    # The Hermes gateway exposes a send subcommand. We shell out best-effort.
    args = [HERMES_BINARY, "send", "--platform", "telegram", "--message", message]
    try:
        proc = subprocess.run(args, capture_output=True, text=True, check=False, timeout=15)
        if proc.returncode != 0:
            print(f"warn: telegram send failed (exit {proc.returncode}): {proc.stderr.strip()}", file=sys.stderr)
    except Exception as exc:
        print(f"warn: telegram send error: {exc}", file=sys.stderr)


# ---------------------------------------------------------------------------
# ALL_DONE detection
# ---------------------------------------------------------------------------


def check_all_done(project_root: Path) -> bool:
    """Return True if 'ALL_DONE' appears in _bmad/logs/progress.txt."""
    p = project_root / "_bmad" / "logs" / "progress.txt"
    if not p.exists():
        return False
    try:
        return "ALL_DONE" in p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False


# ---------------------------------------------------------------------------
# Poll cycle
# ---------------------------------------------------------------------------


def sync_story(
    story: Story,
    state: BridgeState,
    board: str,
    project_root: Path,
    dry_run: bool,
) -> None:
    """Create the card if unseen, then mirror status + QA state."""
    sid = story.story_id
    sstate = state.stories.get(sid)

    # Create if first time we see this story.
    if sstate is None or not sstate.kanban_task_id:
        task_id = hermes_kanban_create(board, story, project_root, dry_run=dry_run)
        if not task_id:
            return
        sstate = StoryState(kanban_task_id=task_id, last_status=story.status)
        state.stories[sid] = sstate
        # If the story is already past triage, PATCH to the right column now.
        col = STATUS_MAP.get(story.status, "triage")
        if col != "running" and col != "triage":
            set_card_status(task_id, col, board, dry_run=dry_run)
        sstate.last_status = story.status
        sstate.last_qa_status = story.qa_status
        return

    # Status transition detected?
    if sstate.last_status != story.status:
        col = STATUS_MAP.get(story.status, "triage")
        if story.status == "COMPLETED":
            result = "QA PASS; bmadder committed and pushed" if story.qa_status == "PASS" else "Story COMPLETED"
            metadata = {"story_id": sid, "qa_status": story.qa_status or "UNKNOWN"}
            hermes_kanban_complete(sstate.kanban_task_id, result, metadata, board, dry_run=dry_run)
            send_telegram(
                f"✅ {sid}: {story.title} — COMPLETED. QA: {story.qa_status or 'UNKNOWN'}.",
                dry_run=dry_run,
            )
        elif col == "running":
            # Cannot PATCH to running; add a comment noting the transition instead.
            hermes_kanban_comment(
                sstate.kanban_task_id,
                f"{sstate.last_status} → {story.status} (cannot move to 'running' via PATCH)",
                board, dry_run=dry_run,
            )
        else:
            if not set_card_status(sstate.kanban_task_id, col, board, dry_run=dry_run):
                # Fallback: at least record the transition as a comment.
                hermes_kanban_comment(
                    sstate.kanban_task_id,
                    f"Status: {sstate.last_status} → {story.status}",
                    board, dry_run=dry_run,
                )
        sstate.last_status = story.status
        sstate.last_qa_status = story.qa_status


def add_log_comment(
    entry: LogEntry,
    state: BridgeState,
    board: str,
    dry_run: bool,
) -> None:
    """Append a log-derived comment to the matching story's card, with dedup."""
    sstate = state.stories.get(entry.story_id)
    if sstate is None or not sstate.kanban_task_id:
        return
    text = entry.to_comment()
    # Include role/model detail when the detail string carries it.
    if "via" in entry.detail:
        text += f"\nRole: {entry.actor} · Model: {entry.detail}"
    fp = fingerprint(text)
    if sstate.last_comment_fingerprint == fp:
        return  # Already posted this exact comment — dedup (AC-3).
    if hermes_kanban_comment(sstate.kanban_task_id, text, board, dry_run=dry_run):
        sstate.last_comment_fingerprint = fp


def poll_once(
    project_root: Path,
    board: str,
    state: BridgeState,
    dry_run: bool,
    jsonl_events: bool = False,
) -> None:
    """One polling pass: discover stories, sync, tail log, persist state.

    When `jsonl_events` is True and `_bmad/logs/events.jsonl` exists, JSONL events
    are processed with higher priority than activity log parsing; story_ids that
    appear in JSONL are skipped during the activity-log tail to avoid duplicate
    comments.
    """
    stories_dir = project_root / "docs" / "backlog" / "stories"
    log_path = project_root / "_bmad" / "logs" / "activity.log"
    jsonl_path = project_root / "_bmad" / "logs" / "events.jsonl"

    # 1. Discover + sync story files.
    stories = list_stories(stories_dir)
    by_id: dict[str, Story] = {s.story_id: s for s in stories}
    for story in stories:
        sync_story(story, state, board, project_root, dry_run)

    # 2a. JSONL events (higher priority when --jsonl-events and file present).
    jsonl_seen: set[str] = set()
    if jsonl_events:
        for ev in read_jsonl_events(jsonl_path):
            sid = ev.get("story_id") or ""
            if not sid or (sid not in by_id and sid not in state.stories):
                continue
            # Use from/to fields for status mapping instead of inferring from frontmatter diff.
            ev_to = ev.get("to") or ""
            ev_event = ev.get("event") or ""
            ev_detail = ev.get("detail") or ""
            ev_actor = ev.get("actor") or "ORCH"
            text = f"{ev_actor} {sid} {ev_event}"
            if ev_to:
                text += f" → {ev_to}"
            if ev_detail:
                text += f" · {ev_detail}"
            entry = LogEntry(
                ts=ev.get("ts", ""),
                actor=ev_actor,
                story_id=sid,
                event=ev_event,
                detail=ev_detail or ev_to,
            )
            add_log_comment(entry, state, board, dry_run)
            jsonl_seen.add(sid)

    # 2b. Activity log tail (skipped for stories already covered by JSONL).
    entries, new_offset = tail_activity_log(log_path, state.activity_log_offset)
    for e in entries:
        if e.story_id in jsonl_seen:
            continue
        # Drop log entries for stories we don't track (e.g. orphans).
        if e.story_id in by_id or e.story_id in state.stories:
            add_log_comment(e, state, board, dry_run)
    state.activity_log_offset = new_offset

    # 3. ALL_DONE -> Telegram completion (fire once, then suppress).
    if check_all_done(project_root) and not state.all_done_notified:
        count = sum(1 for s in stories if s.status == "COMPLETED")
        send_telegram(f"🎉 All stories in {board} COMPLETED ({count} stories)", dry_run=dry_run)
        state.all_done_notified = True
    elif not check_all_done(project_root):
        # Reset so a new run can notify again.
        state.all_done_notified = False

    # 4. Stamp + persist.
    state.last_poll = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    save_state(state_path_for(project_root), state)



# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        prog="bmadder-kanban-bridge",
        description="Passive observer that mirrors BMADder story state into a Hermes Kanban board.",
    )
    parser.add_argument("project_path", help="Path to the BMADder project root (contains bmadder.toml)")
    parser.add_argument("--board", default=None, help="Hermes Kanban board slug (overrides [hermes].project_slug in bmadder.toml)")
    parser.add_argument("--poll", type=int, default=10, help="Poll interval in seconds (default: 10)")
    parser.add_argument("--dry-run", action="store_true", help="Do not call hermes CLI or REST; print actions to stderr")
    parser.add_argument("--once", action="store_true", help="Run a single poll pass and exit")
    parser.add_argument("--jsonl-events", action="store_true", help="Prefer _bmad/logs/events.jsonl over activity.log parsing when present")
    args = parser.parse_args(argv)

    project_root = Path(args.project_path).resolve()
    if not project_root.is_dir():
        print(f"error: project path not a directory: {project_root}", file=sys.stderr)
        return 2

    # Read [hermes] config from bmadder.toml (the file created by `bmadder bootstrap`).
    toml_path = project_root / "bmadder.toml"
    hermes = load_hermes_config(toml_path)
    if not toml_path.exists():
        print(f"warn: no bmadder.toml in {project_root}; proceeding anyway", file=sys.stderr)

    # Resolve board slug: --board overrides config, else config.project_slug, else folder name.
    board = args.board or hermes.board_slug(project_root)

    # Resolve REST base + hermes binary from config (override module defaults).
    global REST_BASE, HERMES_BINARY
    REST_BASE = hermes.rest_base
    HERMES_BINARY = hermes.hermes_binary

    # If bridge_report is false in config and --board wasn't explicitly passed,
    # warn the user — they likely forgot to set bridge_report = true.
    if not hermes.bridge_report and args.board is None:
        print(
            f"warn: [hermes].bridge_report = false in {toml_path}; set it to true to enable reporting. "
            f"Proceeding with board={board!r}.",
            file=sys.stderr,
        )

    # jsonl_events: enable if config says bridge_report (BMADder auto-emits JSONL),
    # OR the user explicitly passed --jsonl-events.
    use_jsonl = args.jsonl_events or hermes.bridge_report

    state_file = state_path_for(project_root)
    state = load_state(state_file, board)
    state.board = board

    print(
        f"🚦 bmadder-bridge started for {board} "
        f"(project={project_root}, poll={args.poll}s, hermes_home={hermes.hermes_home!r}, binary={HERMES_BINARY!r}, jsonl={use_jsonl})",
        file=sys.stderr,
    )

    try:
        while True:
            poll_once(project_root, board, state, args.dry_run, use_jsonl)
            if args.once:
                break
            time.sleep(args.poll)
    except KeyboardInterrupt:
        print("\nbridge stopped by user", file=sys.stderr)
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())