#!/usr/bin/env bash
# sync-to-framework.sh
#
# Syncs the bmadder scripts from this project back to the
# jamespakele/bmadder-framework repo so changes are preserved upstream.
#
# Usage:
#   ./scripts/sync-to-framework.sh
#   ./scripts/sync-to-framework.sh --dry-run   # show what would change

set -euo pipefail

FRAMEWORK_DIR="${BMADDER_FRAMEWORK_DIR:-/home/james/1-projects/bmadder-framework}"
SCRIPTS_SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY_RUN=false

[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=true

# Files to sync from this project's scripts/ → framework/scripts/
SYNC_FILES=(
    bmadder.sh
    bmadder-iterative.sh
    bootstrap_bmadder.py
    preflight_auth.py
    validate_stories.py
    create_rules.py
    init_bmadder.py
    deploy-push.sh
    strip_vault_fences.py
    README.md
)

# ── Validate ─────────────────────────────────────────────────────────────────
if [[ ! -d "$FRAMEWORK_DIR" ]]; then
    echo "ERROR: bmadder-framework not found at $FRAMEWORK_DIR"
    echo "       Set BMADDER_FRAMEWORK_DIR to override."
    exit 1
fi

FRAMEWORK_SCRIPTS="$FRAMEWORK_DIR/scripts"
if [[ ! -d "$FRAMEWORK_SCRIPTS" ]]; then
    echo "ERROR: No scripts/ directory in $FRAMEWORK_DIR"
    exit 1
fi

echo ""
echo "Syncing: $SCRIPTS_SRC → $FRAMEWORK_SCRIPTS"
$DRY_RUN && echo "(DRY RUN — no changes will be made)"
echo ""

# ── Copy files ────────────────────────────────────────────────────────────────
changed=0
for file in "${SYNC_FILES[@]}"; do
    src="$SCRIPTS_SRC/$file"
    dst="$FRAMEWORK_SCRIPTS/$file"

    if [[ ! -f "$src" ]]; then
        echo "  SKIP  $file  (not in this project)"
        continue
    fi

    if [[ -f "$dst" ]] && diff -q "$src" "$dst" >/dev/null 2>&1; then
        echo "  OK    $file  (unchanged)"
        continue
    fi

    echo "  COPY  $file"
    if ! $DRY_RUN; then
        cp "$src" "$dst"
        [[ "$file" == *.sh ]] && chmod +x "$dst"
    fi
    ((changed++)) || true
done

echo ""

if [[ $changed -eq 0 ]]; then
    echo "Nothing changed — framework is already up to date."
    exit 0
fi

if $DRY_RUN; then
    echo "$changed file(s) would be updated."
    exit 0
fi

# ── Commit and push framework ─────────────────────────────────────────────────
echo "Committing changes to bmadder-framework..."
cd "$FRAMEWORK_DIR"

if ! git diff --quiet HEAD -- scripts/ 2>/dev/null; then
    # Grab the latest commit message from iq-kip-v2 for context
    SRC_SHA=$(git -C "$SCRIPTS_SRC/.." rev-parse --short HEAD 2>/dev/null || echo "unknown")
    git add scripts/
    git commit -m "sync: update scripts from iq-kip-v2 @ $SRC_SHA

Updated files ($changed changed):
$(for f in "${SYNC_FILES[@]}"; do
    [[ -f "$SCRIPTS_SRC/$f" ]] && echo "  - $f"
done)"
    git push
    echo ""
    echo "✅ bmadder-framework pushed: $(git rev-parse --short HEAD)"
else
    echo "Nothing to commit in framework (git says clean)."
fi
