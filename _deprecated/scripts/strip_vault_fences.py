#!/usr/bin/env python3
"""
One-time cleanup: strip ```markdown / ``` wrapping from vault .md files.
Updates content_hash in PostgreSQL to match the cleaned file content.

Usage: python3 strip_vault_fences.py
"""

import os
import re
import hashlib
import glob
import psycopg2

DATABASE_URL = os.environ.get("DATABASE_URL", "postgresql://r2v2:r2v2@localhost:5432/r2v2")
VAULT_PATH = os.environ.get("VAULT_PATH", "/home/james/projects/ai-r2v2/r2v2/data/vault")

def sha256_hex(content: str) -> str:
    return hashlib.sha256(content.encode("utf-8")).hexdigest()

def strip_fence(content: str) -> str:
    """Strip a leading ```markdown or ``` fence and trailing ``` from content."""
    s = content.strip()
    # Try ```markdown first, then plain ```
    m = re.match(r'^```markdown\n(.*?)\n```\s*$', s, re.DOTALL)
    if not m:
        m = re.match(r'^```\n(.*?)\n```\s*$', s, re.DOTALL)
    if m:
        return m.group(1)
    return content  # unchanged if no fence found

def main():
    conn = psycopg2.connect(DATABASE_URL)
    cur = conn.cursor()

    pattern = os.path.join(VAULT_PATH, "*.md")
    files = [f for f in glob.glob(pattern) if not f.startswith(os.path.join(VAULT_PATH, "archive"))]

    cleaned = 0
    skipped = 0

    for filepath in sorted(files):
        note_id = os.path.basename(filepath).replace(".md", "")

        with open(filepath, "r", encoding="utf-8") as f:
            original = f.read()

        stripped = strip_fence(original)

        if stripped == original:
            skipped += 1
            continue

        # Write cleaned content
        with open(filepath, "w", encoding="utf-8") as f:
            f.write(stripped)

        # Update DB content_hash
        new_hash = sha256_hex(stripped)
        cur.execute(
            "UPDATE notes SET content_hash = %s, updated_at = NOW() WHERE id = %s",
            (new_hash, note_id)
        )
        print(f"  cleaned: {note_id} ({cur.rowcount} row updated)")
        cleaned += 1

    conn.commit()
    cur.close()
    conn.close()

    print(f"\nDone. {cleaned} files cleaned, {skipped} skipped (no fence).")

if __name__ == "__main__":
    main()
