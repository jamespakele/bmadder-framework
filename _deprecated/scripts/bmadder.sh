#!/usr/bin/env bash
# ============================================================================
# bmadder.sh -- BMADder Framework Orchestrator
# A Ralph Wiggum loop with BMAD state machine gates and role separation.
#
# Design:
#   - Sequential. Stories processed one at a time, in dependency order.
#   - Fresh context per story. Each agent invocation starts clean.
#     The filesystem IS the memory (progress.txt, git history, story files).
#   - Bash is the state machine. It reads frontmatter, validates transitions,
#     and decides what to invoke. The LLM does the work within guardrails.
#
# Agent routing (defaults):
#   plan (SM + PO)   -> claude (sonnet)    Structured reasoning, doc gen
#   dev (all stories)-> codex              Long-horizon coding, build/test
#   qa               -> claude (sonnet)      Deep code review, nuanced decisions
#
#   Stories carry an `agent_hint` frontmatter field set during planning:
#     agent_hint: "codex"    -> backend, API, database, infra, AND frontend
#     agent_hint: "claude"   -> complex logic, config, data transforms
#     agent_hint: "gemini"   -> ONLY if no Stitch scaffolding exists (rare)
#
#   Frontend stories use Stitch design artifacts in src/scaffolding/ as
#   reference. Codex/Claude apply the design; they don't invent it.
#
# Usage:
#   ./scripts/bmadder.sh [phase] [options]
#
# Phases:
#   plan        SM shards PRD -> stories, PO reviews all at once
#   dev         Sequential dev loop, one story at a time, fresh context
#   qa          Sequential QA audit, one story at a time, fresh context
#   cycle       Full pipeline: plan -> dev -> qa (loops back for REFIX)
#   status      Show current story states
#   validate    Run story frontmatter validation only
#
# Options:
#   --max-iter N      Max dev iterations per story (default: 10)
#   --dry-run         Show what would run without executing
#   --skip-po         Skip PO gate (rapid prototyping only)
#   --agent AGENT     Force ALL phases to use this agent (overrides routing)
#   --no-commit       Skip git commit/push after QA pass
#   --timeout SECS    Max seconds per agent invocation (default: 900)
#   --story ID        Target a specific story (e.g., STORY-0001)
#
# Environment overrides:
#   BMADDER_AGENT       Force all phases to one agent
#   BMADDER_MAX_ITER    Max dev iterations (default: 10)
#   BMADDER_STORY_TIMEOUT  Max seconds per agent invocation (default: 900)
#   BMADDER_PLAN_AGENT  Plan phase agent (default: claude)
#   BMADDER_DEV_AGENT   Dev phase default agent (default: codex)
#   BMADDER_QA_AGENT    QA phase agent (default: claude --model opus)
# ============================================================================

set -euo pipefail

# --- Paths ---
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STORIES_DIR="$ROOT/docs/backlog/stories"
STANDARDS_DIR="$ROOT/docs/standards"
BMAD_DIR="$ROOT/_bmad"
HEADLESS_DIR="$ROOT/scripts/headless-skills"
LOG_FILE="$BMAD_DIR/logs/activity.log"
PROGRESS_FILE="$BMAD_DIR/progress.txt"
PROMPT_TMP="$BMAD_DIR/.prompt-tmp.md"

# --- Defaults ---
AGENT_OVERRIDE="${BMADDER_AGENT:-}"
PLAN_AGENT="${BMADDER_PLAN_AGENT:-claude}"
DEV_AGENT="${BMADDER_DEV_AGENT:-claude}"
QA_AGENT="${BMADDER_QA_AGENT:-claude}"
MAX_ITER="${BMADDER_MAX_ITER:-10}"
STORY_TIMEOUT="${BMADDER_STORY_TIMEOUT:-1800}"  # 30 min default per story
DRY_RUN=false
SKIP_PO=false
SKIP_SM=false
NO_COMMIT=false
TARGET_STORY=""
PHASE="${1:-}"
AGENT=""  # Set per-invocation by resolve_agent

# --- Args ---
shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-iter)   MAX_ITER="$2"; shift 2 ;;
        --dry-run)    DRY_RUN=true; shift ;;
        --skip-po)    SKIP_PO=true; shift ;;
        --skip-sm)    SKIP_SM=true; shift ;;
        --agent)      AGENT_OVERRIDE="$2"; shift 2 ;;
        --no-commit)  NO_COMMIT=true; shift ;;
        --timeout)    STORY_TIMEOUT="$2"; shift 2 ;;
        --story)      TARGET_STORY="$2"; shift 2 ;;
        *)            echo "Unknown option: $1"; exit 1 ;;
    esac
done

# --- Colors ---
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; NC='\033[0m'

# --- Logging ---
timestamp() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }
log_activity() {
    mkdir -p "$(dirname "$LOG_FILE")"
    echo "$(timestamp) | $1 | $2 | $3 | $4" >> "$LOG_FILE"
}
log_progress() {
    mkdir -p "$(dirname "$PROGRESS_FILE")"
    echo "$(timestamp) | $*" >> "$PROGRESS_FILE"
}
info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERR]${NC}   $*"; }

# ============================================================================
# STORY HELPERS
# ============================================================================

get_story_field() {
    local file="$1" field="$2"
    sed -n '/^---$/,/^---$/p' "$file" | grep "^${field}:" | head -1 | \
        sed "s/^${field}:[[:space:]]*//" | tr -d '"' | tr -d "'"
}

update_story_field() {
    local file="$1" field="$2" value="$3"
    sed -i "s/^${field}:.*$/${field}: \"${value}\"/" "$file"
}

get_stories_by_status() {
    local status="$1"
    [[ -d "$STORIES_DIR" ]] || return
    for f in "$STORIES_DIR"/story-*.md; do
        [[ -f "$f" ]] || continue
        local s; s=$(get_story_field "$f" "status")
        [[ "$s" != "$status" ]] && continue
        if [[ -n "$TARGET_STORY" ]]; then
            local sid; sid=$(get_story_field "$f" "story_id")
            [[ "$sid" == "$TARGET_STORY" ]] && echo "$f"
        else
            echo "$f"
        fi
    done
}

count_by_status()  { get_stories_by_status "$1" | wc -l | tr -d ' '; }
count_all() {
    local t=0
    for s in DRAFT REVISE READY_FOR_DEV IN_DEV PENDING_QA REFIX COMPLETED; do
        t=$((t + $(count_by_status "$s")))
    done
    echo "$t"
}

# ============================================================================
# AGENT ROUTING
# ============================================================================
# Priority: --agent flag > story agent_hint > phase default
# Plan + QA use Claude (Sonnet for plan, Opus for QA).
# Dev routes per-story: codex (backend), gemini (UI), claude (logic).

resolve_agent() {
    local phase="$1"
    local story_file="${2:-}"

    # Global override from --agent flag or BMADDER_AGENT env
    [[ -n "$AGENT_OVERRIDE" ]] && { echo "$AGENT_OVERRIDE"; return; }

    # Story-level hint (dev phase only)
    if [[ "$phase" == "dev" && -n "$story_file" && -f "$story_file" ]]; then
        local hint; hint=$(get_story_field "$story_file" "agent_hint" 2>/dev/null || echo "")
        [[ -n "$hint" ]] && { echo "$hint"; return; }
    fi

    # Phase defaults
    case "$phase" in
        plan) echo "$PLAN_AGENT" ;;
        dev)  echo "$DEV_AGENT" ;;
        qa)   echo "$QA_AGENT" ;;
        *)    echo "claude" ;;
    esac
}

# Build model flags based on agent + phase
agent_model_flags() {
    local agent="$1" phase="$2"
    case "$agent" in
        claude)
            # sonnet for all phases -- avoids opus limits on long pipeline runs
            echo "--model sonnet"
            ;;
        *) echo "" ;;  # codex and gemini don't need model flags
    esac
}

# ============================================================================
# AGENT INVOCATION
# Fresh context per call. No conversation state carries over.
# ============================================================================

write_prompt() {
    mkdir -p "$(dirname "$PROMPT_TMP")"
    cat > "$PROMPT_TMP"
    echo "$PROMPT_TMP"
}

# Gemini rate-limit backoff state (per-story, reset between stories)
_GEMINI_BACKOFF=30  # seconds, doubles per rate-limited iteration (max 300)

run_agent() {
    local prompt_file="$1"
    local phase="${2:-dev}"  # phase context for model selection

    if $DRY_RUN; then
        info "[DRY RUN] $AGENT ($phase phase)"
        info "Prompt: $(head -3 "$prompt_file")..."
        return 0
    fi

    local model_flags
    model_flags=$(agent_model_flags "$AGENT" "$phase")
    info "-> $AGENT $model_flags (fresh context)"

    # < /dev/null prevents claude/gemini from blocking on stdin after completion
    # codex uses `exec` mode for non-interactive, auto-exit behavior (no TUI).
    # Pre-dev worktree commit ensures codex won't prompt about dirty files.
    # All agents get a timeout to prevent infinite hangs.
    # After codex runs, reset the TTY so the script can continue cleanly.
    local rc=0
    local agent_output_file; agent_output_file=$(mktemp)
    case "$AGENT" in
        claude)  timeout "$STORY_TIMEOUT" claude --dangerously-skip-permissions $model_flags -p "$(cat "$prompt_file")" < /dev/null 2>&1 | tee "$agent_output_file"; rc=${PIPESTATUS[0]} ;;
        codex)
            # Save TTY state, run codex with timeout, restore TTY after
            local tty_settings=""
            tty_settings=$(stty -g 2>/dev/null) || true
            timeout "$STORY_TIMEOUT" codex exec --full-auto "$(cat "$prompt_file")" < /dev/null || rc=$?
            # Restore TTY -- codex can leave terminal in raw mode
            [[ -n "$tty_settings" ]] && stty "$tty_settings" 2>/dev/null || true
            stty sane 2>/dev/null || true
            ;;
        gemini)
            timeout "$STORY_TIMEOUT" gemini --yolo -p "$(cat "$prompt_file")" < /dev/null 2>&1 | tee "$agent_output_file"; rc=${PIPESTATUS[0]}
            ;;
        *)       err "Unknown agent: $AGENT"; exit 1 ;;
    esac

    # Detect gemini rate-limit (429 / MODEL_CAPACITY_EXHAUSTED in output)
    local rate_limited=false
    if [[ "$AGENT" == "gemini" ]] && grep -qiE '429|rateLimitExceeded|MODEL_CAPACITY_EXHAUSTED|No capacity available' "$agent_output_file" 2>/dev/null; then
        rate_limited=true
    fi
    rm -f "$agent_output_file"

    # timeout exit code 124 = killed by timeout
    if [[ $rc -eq 124 ]]; then
        warn "Agent killed by timeout (${STORY_TIMEOUT}s)"
    elif $rate_limited; then
        warn "Gemini rate-limited (429). Backing off ${_GEMINI_BACKOFF}s before next iteration..."
        sleep "$_GEMINI_BACKOFF"
        # Exponential backoff -- cap at 300s (5 min)
        _GEMINI_BACKOFF=$(( _GEMINI_BACKOFF * 2 ))
        [[ $_GEMINI_BACKOFF -gt 300 ]] && _GEMINI_BACKOFF=300
        return 1  # signal caller to retry
    elif [[ $rc -ne 0 ]]; then
        warn "Agent exited with code $rc"
    fi

    # Polite cooldown between gemini calls to stay under quota
    if [[ "$AGENT" == "gemini" && $rc -eq 0 ]]; then
        info "  [gemini cooldown 15s]"
        sleep 15
    fi

    return $rc
}

# ============================================================================
# VALIDATION
# ============================================================================

validate_stories() {
    info "Validating story frontmatter..."
    if command -v uv &>/dev/null && [[ -f "$ROOT/scripts/validate_stories.py" ]]; then
        uv run "$ROOT/scripts/validate_stories.py"
        return
    fi
    # Inline fallback
    local errors=0
    for f in "$STORIES_DIR"/story-*.md; do
        [[ -f "$f" ]] || continue
        local s; s=$(get_story_field "$f" "status")
        case "$s" in
            DRAFT|REVISE|READY_FOR_DEV|IN_DEV|PENDING_QA|REFIX|COMPLETED) ;;
            *) err "  $(basename "$f"): invalid status '$s'"; ((errors++)) || true ;;
        esac
    done
    [[ $errors -eq 0 ]] && ok "All stories valid." || err "$errors invalid stories."
}

# ============================================================================
# STATUS
# ============================================================================

show_status() {
    echo ""
    echo -e "${CYAN}===================================================${NC}"
    echo -e "${CYAN}  BMADder Status${NC}"
    echo -e "${CYAN}===================================================${NC}"
    echo ""
    for s in DRAFT REVISE READY_FOR_DEV IN_DEV PENDING_QA REFIX COMPLETED; do
        local c; c=$(count_by_status "$s")
        case "$s" in
            COMPLETED)     echo -e "  ${GREEN}$s${NC}      $c" ;;
            REFIX)         echo -e "  ${RED}$s${NC}          $c" ;;
            READY_FOR_DEV) echo -e "  ${BLUE}$s${NC}  $c" ;;
            IN_DEV)        echo -e "  ${BLUE}$s${NC}         $c" ;;
            *)             echo -e "  ${YELLOW}$s${NC}         $c" ;;
        esac
    done
    echo -e "\n  Total: $(count_all)"
    echo ""
    echo -e "${CYAN}  Key Files:${NC}"
    [[ -f "$ROOT/docs/prd.md" ]]              && echo -e "  ${GREEN}[OK]${NC} docs/prd.md"                || echo -e "  ${RED}[X]${NC} docs/prd.md"
    [[ -f "$ROOT/docs/architecture.md" ]]      && echo -e "  ${GREEN}[OK]${NC} docs/architecture.md"        || echo -e "  ${RED}[X]${NC} docs/architecture.md"
    [[ -f "$BMAD_DIR/orchestrator-master.md" ]] && echo -e "  ${GREEN}[OK]${NC} _bmad/orchestrator-master.md" || echo -e "  ${RED}[X]${NC} _bmad/orchestrator-master.md"
    [[ -f "$PROGRESS_FILE" ]]                  && echo -e "  ${GREEN}[OK]${NC} _bmad/progress.txt"           || echo -e "  ${YELLOW}-${NC} _bmad/progress.txt"
    echo ""
    echo -e "${CYAN}  Agent Routing:${NC}"
    echo -e "  plan -> ${PLAN_AGENT} (sonnet)    dev -> ${DEV_AGENT}    qa -> ${QA_AGENT} (sonnet)"
    [[ -n "$AGENT_OVERRIDE" ]] && echo -e "  ${YELLOW}[!] Global override: $AGENT_OVERRIDE${NC}"
    echo ""
}

# ============================================================================
# PHASE: PLAN
# Two sequential invocations, both use Claude Sonnet:
#   1. SM: reads PRD + arch -> creates story files
#   2. PO: reads ALL drafts at once -> approves or revises
# ============================================================================

run_plan() {
    echo ""
    echo -e "${CYAN}===================================================${NC}"
    echo -e "${CYAN}  Phase: PLAN (SM -> Stories -> PO Gate)${NC}"
    echo -e "${CYAN}===================================================${NC}"
    echo ""

    [[ -f "$ROOT/docs/prd.md" ]]         || { err "docs/prd.md missing."; exit 1; }
    [[ -f "$ROOT/docs/architecture.md" ]] || { err "docs/architecture.md missing."; exit 1; }

    # --- SM ---
    if $SKIP_SM; then
        warn "Skipping SM (--skip-sm). Using existing stories."
        log_activity "ORCH" "-" "SM_SKIP" "SM skipped, using existing stories"
        validate_stories
        local drafts; drafts=$(count_by_status "DRAFT")
        info "$drafts existing DRAFT stories found."
        [[ $drafts -eq 0 ]] && { err "No DRAFT stories found. Run without --skip-sm first."; exit 1; }
    else
        AGENT=$(resolve_agent "plan")
        info "Step 1/2: Scrum Master [$AGENT sonnet]"
        log_activity "ORCH" "-" "SM_START" "SM sharding via $AGENT"

        local pf
        pf=$(write_prompt <<'EOF'
You are the Scrum Master running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated headless skill instructions:
@scripts/headless-skills/sm-create-stories.md

Context documents:
@docs/prd.md
@docs/architecture.md

Pipeline rules:
- Stories go in: docs/backlog/stories/story-NNNN-slug.md
- Use YAML frontmatter with: status: "DRAFT", po_alignment: "PENDING"
- Each story MUST have sections: Context, Requirements, Acceptance Criteria,
  Implementation Notes, PO Alignment, QA Notes.

Pre-check:
BEFORE creating stories, list existing files in docs/backlog/stories/.
Do NOT recreate existing stories. SKIP stories with status: "READY_FOR_DEV" or "COMPLETED".
Only work on stories with status: "REVISE" or stories that don't exist yet.

Revision handling:
For stories with status: "REVISE":
1. Read ## PO Alignment for revision notes.
2. Address every issue. Update content. Set status: "DRAFT", po_alignment: "PENDING".
3. Append dated note under ## PO Alignment.

If no MISSING or REVISE stories remain, log that sharding is complete and exit.

Do NOT implement code. Do NOT approve stories.
Log a summary to _bmad/logs/activity.log.
EOF
        )
        run_agent "$pf" "plan"
        log_activity "SM" "-" "SM_DONE" "Sharding complete"
        ok "SM sharding complete."

        validate_stories
        local drafts; drafts=$(count_by_status "DRAFT")
        info "$drafts DRAFT stories created."
        [[ $drafts -eq 0 ]] && { err "No stories created. Check output."; exit 1; }
    fi

    # --- PO ---
    if $SKIP_PO; then
        warn "Skipping PO gate (--skip-po). Auto-approving all DRAFTs."
        for f in "$STORIES_DIR"/story-*.md; do
            [[ -f "$f" ]] || continue
            [[ "$(get_story_field "$f" "status")" == "DRAFT" ]] || continue
            update_story_field "$f" "status" "READY_FOR_DEV"
            update_story_field "$f" "po_alignment" "APPROVED"
        done
        log_activity "ORCH" "-" "PO_SKIP" "All drafts auto-approved"
    else
        AGENT=$(resolve_agent "plan")
        info "Step 2/2: Product Owner [$AGENT sonnet]"
        log_activity "ORCH" "-" "PO_START" "PO review via $AGENT"

        pf=$(write_prompt <<'EOF'
You are the Product Owner running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the story quality checklist:
@scripts/headless-skills/po-review.md

Context documents:
@docs/prd.md
@docs/architecture.md

Read EVERY story in docs/backlog/stories/ with status: "DRAFT".

For each draft story, evaluate against the checklist criteria above PLUS:
1. Does it map to at least one PRD requirement?
2. Is it consistent with the architecture?
3. Are Requirements and Acceptance Criteria clear, specific, testable?
4. Is scope small enough for one implementation + testing effort?
5. Are there dependency gaps (assumes work from a missing story)?

If ALL criteria pass:
- Set status: "READY_FOR_DEV", po_alignment: "APPROVED"
- Append dated approval note under ## PO Alignment

If ANY criterion fails:
- Set status: "REVISE", po_alignment: "REVISE"
- Append specific revision notes under ## PO Alignment

Log decisions to _bmad/logs/activity.log.
Do NOT move any story to IN_DEV or PENDING_QA.
EOF
        )
        run_agent "$pf" "plan"
        log_activity "PO" "-" "PO_DONE" "Review complete"
        ok "PO review complete."
    fi

    local ready; ready=$(count_by_status "READY_FOR_DEV")
    local revise; revise=$(count_by_status "REVISE")
    info "Result: $ready READY_FOR_DEV, $revise REVISE"
    log_progress "PLAN: $ready approved, $revise need revision"
}

# ============================================================================
# PHASE: DEV
# Sequential Ralph loop. One story at a time.
# Each iteration = fresh agent context.
# Agent is routed per-story via agent_hint (codex default, gemini for UI).
# Bash checks PENDING_QA status on disk after each iteration.
# ============================================================================

run_dev() {
    echo ""
    echo -e "${CYAN}===================================================${NC}"
    echo -e "${CYAN}  Phase: DEV (Sequential, Fresh Context per Story)${NC}"
    echo -e "${CYAN}===================================================${NC}"
    echo ""

    # Clean worktree before dev loop to prevent agent stdin prompts
    if ! git diff --quiet HEAD 2>/dev/null || [ -n "$(git ls-files --others --exclude-standard)" ]; then
        info "Committing uncommitted files before dev loop..."
        git add -A && git commit -m "chore: pre-dev worktree snapshot" || true
    fi

    # Queue: READY_FOR_DEV first (in filename order), then REFIX
    local stories=()
    while IFS= read -r l; do [[ -n "$l" ]] && stories+=("$l"); done < <(get_stories_by_status "READY_FOR_DEV")
    while IFS= read -r l; do [[ -n "$l" ]] && stories+=("$l"); done < <(get_stories_by_status "REFIX")

    if [[ ${#stories[@]} -eq 0 ]]; then
        warn "No READY_FOR_DEV or REFIX stories. Nothing to develop."
        return 0
    fi
    info "${#stories[@]} stories queued."

    for story_file in "${stories[@]}"; do
        [[ -f "$story_file" ]] || continue
        local story_id title
        story_id=$(get_story_field "$story_file" "story_id")
        title=$(get_story_field "$story_file" "title")

        # Resolve agent for THIS story (checks agent_hint)
        AGENT=$(resolve_agent "dev" "$story_file")
        local hint; hint=$(get_story_field "$story_file" "agent_hint" 2>/dev/null || echo "default")

        echo ""
        echo -e "${CYAN}--- $story_id: $title [$AGENT / $hint] ---${NC}"

        # Reset gemini backoff at start of each story
        _GEMINI_BACKOFF=30
        update_story_field "$story_file" "status" "IN_DEV"
        log_activity "ORCH" "$story_id" "DEV_START" "Dev loop via $AGENT"

        local iter=0
        while [[ $iter -lt $MAX_ITER ]]; do
            (( iter++ )) || true
            info "  Iteration $iter/$MAX_ITER [$AGENT]"

            local pf
            pf=$(write_prompt <<PROMPT_EOF
You are the Developer running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated dev workflow:
@scripts/headless-skills/dev-story.md

Working on ONE story:
  ID: $story_id
  File: @$story_file

Context:
@docs/architecture.md
@docs/prd.md
@_bmad/progress.txt

Also run: \`git log --oneline -20\` to see what previous iterations built.

Completion criteria:
- When build/test/lint pass AND all acceptance criteria are met:
  - Update story frontmatter: status: "PENDING_QA"
  - Fill in ## Implementation Notes: files changed, approach, decisions
- Append to _bmad/progress.txt: what you did, files modified, decisions, notes for QA
- Commit: \`git add -A && git commit -m "feat($story_id): <summary>"\`

Rules:
- ONLY work on $story_id. Do not touch other stories.
- Do NOT skip feedback loops.
- If you can't finish this iteration, commit partial progress, update
  progress.txt, and leave status "IN_DEV". Next iteration picks up.
PROMPT_EOF
            )

            run_agent "$pf" "dev" || true

            # Bash checks status on disk -- not trusting agent output
            local st; st=$(get_story_field "$story_file" "status")
            if [[ "$st" == "PENDING_QA" ]]; then
                ok "  $story_id -> PENDING_QA (iteration $iter)"
                log_activity "DEV" "$story_id" "DEV_DONE" "$iter iterations via $AGENT"
                log_progress "$story_id: DEV done, $iter iters, $AGENT"
                break
            fi
            info "  $story_id still IN_DEV. Continuing..."
            log_progress "$story_id: iter $iter -- in progress ($AGENT)"

            # Inter-iteration delay for gemini to respect quota
            # Skipped on last iteration to avoid pointless wait before stall check
            if [[ "$AGENT" == "gemini" && $iter -lt $MAX_ITER ]]; then
                local st2; st2=$(get_story_field "$story_file" "status")
                if [[ "$st2" != "PENDING_QA" ]]; then
                    info "  [gemini inter-iter pause 20s]"
                    sleep 20
                fi
            fi
        done

        # Max iter check
        local final; final=$(get_story_field "$story_file" "status")
        if [[ "$final" != "PENDING_QA" && "$final" != "COMPLETED" ]]; then
            err "  $story_id stalled after $MAX_ITER iterations ($final)."
            log_activity "DEV" "$story_id" "DEV_STALLED" "Max iter ($MAX_ITER)"
            log_progress "$story_id: STALLED $MAX_ITER iters"
        fi
    done
}

# ============================================================================
# PHASE: QA
# Sequential. One story at a time. Fresh context. Claude Opus.
# Bash enforces git commit on PASS, forces REFIX on ambiguous results.
# ============================================================================

run_qa() {
    echo ""
    echo -e "${CYAN}===================================================${NC}"
    echo -e "${CYAN}  Phase: QA (Sequential, Claude Opus)${NC}"
    echo -e "${CYAN}===================================================${NC}"
    echo ""

    local stories=()
    while IFS= read -r l; do [[ -n "$l" ]] && stories+=("$l"); done < <(get_stories_by_status "PENDING_QA")

    if [[ ${#stories[@]} -eq 0 ]]; then
        warn "No PENDING_QA stories."
        return 0
    fi
    info "${#stories[@]} stories queued for QA."

    for story_file in "${stories[@]}"; do
        [[ -f "$story_file" ]] || continue
        local story_id title
        story_id=$(get_story_field "$story_file" "story_id")
        title=$(get_story_field "$story_file" "title")

        AGENT=$(resolve_agent "qa" "$story_file")
        echo ""
        info "QA: $story_id -- $title [$AGENT sonnet]"
        log_activity "ORCH" "$story_id" "QA_START" "QA via $AGENT sonnet"

        local pf
        pf=$(write_prompt <<PROMPT_EOF
You are the QA Auditor running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated code review workflow:
@scripts/headless-skills/qa-review.md

Auditing ONE story:
  ID: $story_id
  File: @$story_file

Context:
@docs/prd.md
@docs/architecture.md

Task:
1. Read the story's Requirements, Acceptance Criteria, Implementation Notes.
2. Review the code files referenced in Implementation Notes.
3. Run the test suite.
4. Verify each acceptance criterion against the implementation.
5. Check for regressions vs PRD and architecture.

If ALL checks pass:
- Update story: qa_status: "PASS", status: "COMPLETED"
- Append under ## QA Notes: what you tested, how, residual risks
- Do NOT run git commit (the orchestrator handles that)

If ANY check fails:
- Update story: qa_status: "FAIL", status: "REFIX"
- Append under ## QA Notes: what failed, steps to reproduce, fix guidance
- Do NOT commit

Log to _bmad/logs/activity.log.
PROMPT_EOF
        )

        run_agent "$pf" "qa"

        # Bash enforces outcomes
        local ns; ns=$(get_story_field "$story_file" "status")
        if [[ "$ns" == "COMPLETED" ]]; then
            ok "  $story_id: QA PASS"
            log_activity "QA" "$story_id" "QA_PASS" "Completed"
            log_progress "$story_id: QA PASS"

            if ! $NO_COMMIT && ! $DRY_RUN; then
                git add -A 2>/dev/null || true
                git commit -m "story($story_id): $title [QA PASS]" 2>/dev/null || true
                if git push 2>/dev/null; then
                    log_activity "QA" "$story_id" "GIT_PUSH" "ok"
                else
                    warn "  git push failed"
                    log_activity "QA" "$story_id" "GIT_PUSH_FAIL" "push failed"
                fi
            fi

        elif [[ "$ns" == "REFIX" ]]; then
            warn "  $story_id: QA FAIL -> REFIX"
            log_activity "QA" "$story_id" "QA_FAIL" "Needs refix"
            log_progress "$story_id: QA FAIL"

        else
            warn "  $story_id: QA ambiguous (status=$ns). Forcing REFIX."
            update_story_field "$story_file" "status" "REFIX"
            update_story_field "$story_file" "qa_status" "FAIL"
            log_activity "QA" "$story_id" "QA_FORCED_REFIX" "Ambiguous result"
        fi
    done
}

# ============================================================================
# PHASE: CYCLE
# Full pipeline. plan -> dev -> qa. REFIX loops back to dev.
# ============================================================================

run_cycle() {
    echo ""
    echo -e "${CYAN}=========================================================${NC}"
    echo -e "${CYAN}  BMADder Cycle: PLAN -> DEV -> QA${NC}"
    echo -e "${CYAN}=========================================================${NC}"
    echo ""
    log_activity "ORCH" "-" "CYCLE_START" "Full cycle"

    # Plan if nothing is actionable
    local ready; ready=$(count_by_status "READY_FOR_DEV")
    local refix; refix=$(count_by_status "REFIX")
    local drafts; drafts=$(count_by_status "DRAFT")
    local revise; revise=$(count_by_status "REVISE")
    if [[ $ready -eq 0 && $refix -eq 0 ]]; then
        if [[ $drafts -gt 0 && $revise -eq 0 ]]; then
            # DRAFTs exist, no revisions needed -- skip SM, run PO only
            info "$drafts DRAFT stories exist. Skipping SM, running PO review..."
            SKIP_SM=true
        elif [[ $revise -gt 0 ]]; then
            # REVISE stories need SM attention before PO re-review
            info "$revise REVISE stories need SM fixes. Running SM then PO..."
        fi
        run_plan
    fi

    # Dev -> QA with REFIX loop
    local pass=0 max_passes=3
    while [[ $pass -lt $max_passes ]]; do
        ((pass++)) || true
        echo ""
        info "=== Dev/QA pass $pass/$max_passes ==="
        run_dev
        run_qa

        refix=$(count_by_status "REFIX")
        [[ $refix -eq 0 ]] && break
        warn "$refix stories need REFIX. Looping..."
        log_progress "Pass $pass: $refix REFIX"
    done

    # Report
    echo ""
    show_status
    local done_count; done_count=$(count_by_status "COMPLETED")
    local total; total=$(count_all)

    echo ""
    if [[ $done_count -eq $total && $total -gt 0 ]]; then
        echo -e "${GREEN}===================================================${NC}"
        echo -e "${GREEN}  ALL $total STORIES COMPLETED${NC}"
        echo -e "${GREEN}===================================================${NC}"
        log_activity "ORCH" "-" "CYCLE_DONE" "All $total done"
    else
        echo -e "${YELLOW}===================================================${NC}"
        echo -e "${YELLOW}  $done_count/$total completed${NC}"
        echo -e "${YELLOW}===================================================${NC}"
        local stalled; stalled=$(count_by_status "REFIX")
        local stuck; stuck=$(count_by_status "IN_DEV")
        [[ $stalled -gt 0 ]] && warn "  $stalled REFIX"
        [[ $stuck -gt 0 ]]   && warn "  $stuck IN_DEV (stalled)"
        log_activity "ORCH" "-" "CYCLE_PARTIAL" "$done_count/$total"
    fi
}

# ============================================================================
# MAIN
# ============================================================================

# ============================================================================
# PREFLIGHT: Auth + billing safety
# Runs before any phase that invokes agents. Skipped for status/validate.
# ============================================================================

preflight() {
    if $DRY_RUN; then return 0; fi

    # Determine which agents this run needs
    local agents=()
    case "$PHASE" in
        plan)  agents=("$PLAN_AGENT") ;;
        dev)   agents=("$DEV_AGENT") ;;
        qa)    agents=("$QA_AGENT") ;;
        cycle) agents=("$PLAN_AGENT" "$DEV_AGENT" "$QA_AGENT") ;;
    esac
    [[ -n "$AGENT_OVERRIDE" ]] && agents=("$AGENT_OVERRIDE")

    # Deduplicate
    local unique_agents=()
    local seen=""
    for a in "${agents[@]}"; do
        if [[ ! "$seen" == *"$a"* ]]; then
            unique_agents+=("$a")
            seen="$seen $a"
        fi
    done

    # Run Python preflight if available
    local preflight_script="$ROOT/scripts/preflight_auth.py"
    if [[ -f "$preflight_script" ]]; then
        info "Running auth preflight..."
        local runner="python3"
        command -v uv &>/dev/null && runner="uv run"
        if ! $runner "$preflight_script" --agents "${unique_agents[@]}"; then
            err "Auth preflight failed. Fix issues above or skip with --dry-run."
            exit 1
        fi
    else
        # Fallback: minimal inline check for rogue env vars
        local rogue=false
        for a in "${unique_agents[@]}"; do
            case "$a" in
                claude)
                    [[ -n "${ANTHROPIC_API_KEY:-}" ]] && {
                        warn "ANTHROPIC_API_KEY is set -- Claude will bill API instead of subscription"
                        rogue=true
                    }
                    ;;
                codex)
                    [[ -n "${OPENAI_API_KEY:-}" ]] && {
                        warn "OPENAI_API_KEY is set -- Codex will bill API instead of subscription"
                        rogue=true
                    }
                    ;;
                gemini)
                    [[ -n "${GEMINI_API_KEY:-}${GOOGLE_API_KEY:-}" ]] && {
                        warn "GEMINI_API_KEY or GOOGLE_API_KEY is set -- Gemini will bill API instead of subscription"
                        rogue=true
                    }
                    ;;
            esac
        done
        $rogue && warn "Unset rogue vars or run: uv run scripts/preflight_auth.py --fix"
    fi
}

main() {
    [[ -f "$BMAD_DIR/orchestrator-master.md" ]] || {
        err "Not a BMADder project. Run: uv run scripts/bootstrap_bmadder.py"
        exit 1
    }

    # Run auth preflight for phases that invoke agents
    case "$PHASE" in
        plan|dev|qa|cycle) preflight ;;
    esac

    case "$PHASE" in
        plan)     run_plan ;;
        dev)      run_dev ;;
        qa)       run_qa ;;
        cycle)    run_cycle ;;
        status)   show_status ;;
        validate) validate_stories ;;
        "")
            show_status
            echo "  Usage: ./scripts/bmadder.sh [plan|dev|qa|cycle|status|validate]"
            echo ""
            ;;
        *)
            err "Unknown: $PHASE"
            echo "  Usage: ./scripts/bmadder.sh [plan|dev|qa|cycle|status|validate]"
            exit 1
            ;;
    esac
}

main
