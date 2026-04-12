"""
validate_stories.py — BMADder story frontmatter validator.
Checks all stories for valid YAML frontmatter, required fields,
valid state machine values, and required markdown sections.

Usage:
  uv run scripts/validate_stories.py          # validate only
  uv run scripts/validate_stories.py --fix    # fix missing sections, then validate
"""

import argparse
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


# --- Section stubs for --fix mode ---
SECTION_STUBS = {
    "## Context": "## Context\n\n_TODO: Add context._\n",
    "## Requirements": "## Requirements\n\n_TODO: Add requirements._\n",
    "## Acceptance Criteria": "## Acceptance Criteria\n\n_TODO: Add acceptance criteria._\n",
    "## Implementation Notes": "## Implementation Notes\n\n_To be filled during development._\n",
    "## PO Alignment": "## PO Alignment\n\n_Pending PO review._\n",
    "## QA Notes": "## QA Notes\n\n_To be filled during QA._\n",
}


def fix_story(path: Path) -> list[str]:
    """Insert missing required sections in canonical order. Returns list of fixes applied."""
    text = path.read_text(encoding="utf-8")
    fixes = []

    # Find which sections are missing
    missing = [s for s in REQUIRED_SECTIONS if s not in text]
    if not missing:
        return fixes

    # For each missing section, insert it before the next existing section
    # in the canonical order, or at end of file.
    for section in missing:
        # Re-read text each iteration since we're modifying it
        text = path.read_text(encoding="utf-8")

        idx = REQUIRED_SECTIONS.index(section)
        insert_before = None

        # Find the next section that exists in the file
        for later in REQUIRED_SECTIONS[idx + 1:]:
            if later in text:
                insert_before = later
                break

        stub = SECTION_STUBS[section]

        if insert_before:
            # Insert before the next existing section
            text = text.replace(insert_before, stub + "\n" + insert_before)
        else:
            # Append at end of file
            if not text.endswith("\n"):
                text += "\n"
            text += "\n" + stub

        path.write_text(text, encoding="utf-8")
        fixes.append(f"{path.name}: added '{section}'")

    return fixes


def main():
    parser = argparse.ArgumentParser(description="Validate BMADder story files.")
    parser.add_argument("--fix", action="store_true",
                        help="Auto-insert missing required sections before validating")
    args = parser.parse_args()

    if not STORIES_DIR.exists():
        print(f"[WARN] {STORIES_DIR} does not exist. No stories to validate.")
        return

    stories = sorted(STORIES_DIR.glob("story-*.md"))
    if not stories:
        print("[WARN] No story files found.")
        return

    # --- Fix pass ---
    if args.fix:
        total_fixes = 0
        for story in stories:
            fixes = fix_story(story)
            for f in fixes:
                print(f"  [FIX]  {f}")
            total_fixes += len(fixes)
        if total_fixes:
            print(f"\n[FIX] Applied {total_fixes} fixes.\n")
        else:
            print("[OK] Nothing to fix.\n")

    # --- Validate pass ---
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
