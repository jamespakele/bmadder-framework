"""
validate_stories.py — BMADder story frontmatter validator.
Checks all stories for valid YAML frontmatter, required fields,
valid state machine values, and required markdown sections.

Usage: uv run scripts/validate_stories.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STORIES_DIR = ROOT / "docs" / "backlog" / "stories"

# Valid values per the state machine in orchestrator-master.md
VALID_STATUSES = {"DRAFT", "REVISE", "READY_FOR_DEV", "IN_DEV", "PENDING_QA", "REFIX", "COMPLETED"}
VALID_PO_ALIGNMENT = {"PENDING", "APPROVED", "REVISE"}
VALID_QA_STATUS = {"NOT_STARTED", "PASS", "FAIL"}
VALID_AGENT_HINTS = {"codex", "gemini", "claude", ""}  # empty = default
VALID_PRIORITIES = {"MUST_HAVE", "SHOULD_HAVE", "COULD_HAVE", "WONT_HAVE", ""}

REQUIRED_FIELDS = ["story_id", "title", "status"]
REQUIRED_SECTIONS = ["## Context", "## Requirements", "## Acceptance Criteria",
                     "## Implementation Notes", "## PO Alignment", "## QA Notes"]

# Valid state transitions
VALID_TRANSITIONS = {
    "DRAFT":         {"REVISE", "READY_FOR_DEV"},
    "REVISE":        {"DRAFT"},
    "READY_FOR_DEV": {"IN_DEV"},
    "IN_DEV":        {"PENDING_QA"},
    "PENDING_QA":    {"COMPLETED", "REFIX"},
    "REFIX":         {"IN_DEV"},
    "COMPLETED":     set(),  # terminal
}


def parse_frontmatter(text: str) -> dict[str, str]:
    """Extract YAML frontmatter fields as a flat dict."""
    match = re.match(r"^---\n(.*?)\n---", text, re.DOTALL)
    if not match:
        return {}
    fields = {}
    for line in match.group(1).splitlines():
        if ":" in line:
            key, _, val = line.partition(":")
            val = val.strip().strip('"').strip("'")
            fields[key.strip()] = val
    return fields


def validate_story(path: Path) -> list[str]:
    """Validate a single story file. Returns list of error strings."""
    errors = []
    name = path.name
    text = path.read_text(encoding="utf-8")

    # --- Frontmatter ---
    fm = parse_frontmatter(text)
    if not fm:
        return [f"{name}: no YAML frontmatter found (missing --- delimiters)"]

    for field in REQUIRED_FIELDS:
        if field not in fm or not fm[field]:
            errors.append(f"{name}: missing required field '{field}'")

    status = fm.get("status", "")
    if status and status not in VALID_STATUSES:
        errors.append(f"{name}: invalid status '{status}' (valid: {', '.join(sorted(VALID_STATUSES))})")

    po = fm.get("po_alignment", "")
    if po and po not in VALID_PO_ALIGNMENT:
        errors.append(f"{name}: invalid po_alignment '{po}'")

    qa = fm.get("qa_status", "")
    if qa and qa not in VALID_QA_STATUS:
        errors.append(f"{name}: invalid qa_status '{qa}'")

    hint = fm.get("agent_hint", "")
    if hint and hint not in VALID_AGENT_HINTS:
        errors.append(f"{name}: invalid agent_hint '{hint}' (valid: codex, gemini, claude)")

    priority = fm.get("priority", "")
    if priority and priority not in VALID_PRIORITIES:
        errors.append(f"{name}: invalid priority '{priority}'")

    # --- Consistency checks ---
    if status == "READY_FOR_DEV" and po != "APPROVED":
        errors.append(f"{name}: status is READY_FOR_DEV but po_alignment is '{po}' (should be APPROVED)")

    if status == "COMPLETED" and fm.get("qa_status", "") != "PASS":
        errors.append(f"{name}: status is COMPLETED but qa_status is '{fm.get('qa_status', '')}' (should be PASS)")

    # --- Required sections ---
    for section in REQUIRED_SECTIONS:
        if section not in text:
            errors.append(f"{name}: missing section '{section}'")

    # --- Filename convention ---
    if not re.match(r"story-\d{4}-", name):
        errors.append(f"{name}: filename should match story-NNNN-slug.md")

    return errors


def main():
    if not STORIES_DIR.exists():
        print(f"[WARN] {STORIES_DIR} does not exist. No stories to validate.")
        return

    stories = sorted(STORIES_DIR.glob("story-*.md"))
    if not stories:
        print("[WARN] No story files found.")
        return

    total_errors = 0
    for story in stories:
        errors = validate_story(story)
        if errors:
            for e in errors:
                print(f"  [FAIL] {e}")
            total_errors += len(errors)
        else:
            print(f"  [OK]   {story.name}")

    print()
    if total_errors == 0:
        print(f"[OK] All {len(stories)} stories valid.")
    else:
        print(f"[FAIL] {total_errors} errors across {len(stories)} stories.")
        sys.exit(1)


if __name__ == "__main__":
    main()
