#!/usr/bin/env bash
# ============================================================================
# bmadder_iterative.sh — BMADder Iterative Story Pipeline
#
# Philosophy:
#   Process ONE story at a time through the COMPLETE pipeline before moving
#   to the next. Each completed story = a working, deployable increment.
#   Functionality accumulates as an MVP, story by story.
#
# Pipeline per story:
#   1. SM: Create / refine the story
#   2. PO: Verify story → if revisions needed, loop back to SM
#      (repeat SM → PO until PO approves)
#   3. Dev: Implement story
#   4. QA: Review implementation → if issues found, loop back to Dev
#      (repeat Dev → QA until QA passes)
#   5. git commit → move on to next story
#
# vs bmadder.sh cycle:
#   bmadder.sh cycle  → plans ALL stories first, then devs ALL, then QAs ALL
#   bmadder_iterative → for each story: (SM↔PO loop) → (Dev↔QA loop) → commit
#
# Usage:
#   ./scripts/bmadder_iterative.sh [options]
#
# Options:
#   --max-sm-iter N     Max SM↔PO loops per story (default: 5)
#   --max-dev-iter N    Max Dev↔QA loops per story (default: 10)
#   --dry-run           Show what would run without executing
#   --skip-po           Skip PO gate (rapid prototyping only)
#   --skip-plan         Skip auto-plan bootstrap even if no stories exist
#   --agent AGENT       Force ALL phases to use this agent (claude|codex|gemini|opencode)
#   --no-commit         Skip git commit after each story QA pass
#   --timeout SECS      Max seconds per agent invocation (default: 1800)
#   --story ID          Process only this story (e.g., STORY-0001)
#   --start-from ID     Skip stories before this ID (resume from mid-backlog)
#   --from-existing     Skip SM/PO loop; use existing READY_FOR_DEV stories
#
# Environment overrides:
#   BMADDER_AGENT            Force all phases to one agent
#   BMADDER_MAX_SM_ITER      Max SM↔PO loops (default: 5)
#   BMADDER_MAX_DEV_ITER     Max Dev↔QA loops (default: 10)
#   BMADDER_STORY_TIMEOUT    Max seconds per agent invocation (default: 1800)
#   BMADDER_PLAN_AGENT       SM/PO agent (default: claude)
#   BMADDER_DEV_AGENT        Dev agent default (default: codex)
#   BMADDER_QA_AGENT         QA agent (default: claude)
# ============================================================================

set -euo pipefail

# --- Paths ---
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STORIES_DIR="$ROOT/docs/backlog/stories"
BMAD_DIR="$ROOT/_bmad"
LOG_FILE="$BMAD_DIR/logs/activity.log"
PROGRESS_FILE="$BMAD_DIR/progress.txt"
PROMPT_TMP="$BMAD_DIR/.prompt-itmp.md"

# --- Defaults ---
AGENT_OVERRIDE="${BMADDER_AGENT:-}"
PLAN_AGENT="${BMADDER_PLAN_AGENT:-claude}"
DEV_AGENT="${BMADDER_DEV_AGENT:-claude}"
QA_AGENT="${BMADDER_QA_AGENT:-claude}"
MAX_SM_ITER="${BMADDER_MAX_SM_ITER:-5}"
MAX_DEV_ITER="${BMADDER_MAX_DEV_ITER:-10}"
STORY_TIMEOUT="${BMADDER_STORY_TIMEOUT:-1800}"
DRY_RUN=false
SKIP_PO=false
SKIP_PLAN=false
NO_COMMIT=false
FROM_EXISTING=false
TARGET_STORY=""
START_FROM=""
AGENT=""

# --- Args ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-sm-iter)   MAX_SM_ITER="$2";  shift 2 ;;
        --max-dev-iter)  MAX_DEV_ITER="$2"; shift 2 ;;
        --dry-run)       DRY_RUN=true;      shift ;;
        --skip-po)       SKIP_PO=true;      shift ;;
        --skip-plan)     SKIP_PLAN=true;    shift ;;
        --agent)         AGENT_OVERRIDE="$2"; shift 2 ;;
        --no-commit)     NO_COMMIT=true;    shift ;;
        --timeout)       STORY_TIMEOUT="$2"; shift 2 ;;
        --story)         TARGET_STORY="$2"; shift 2 ;;
        --start-from)    START_FROM="$2";   shift 2 ;;
        --from-existing) FROM_EXISTING=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

# --- Colors ---
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; MAGENTA='\033[0;35m'; NC='\033[0m'

# --- Logging ---
timestamp()    { date -u +"%Y-%m-%dT%H:%M:%SZ"; }
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
phase() { echo -e "\n${MAGENTA}$*${NC}"; }
story_banner() {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║  $*${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
}

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

count_by_status() { get_stories_by_status "$1" | wc -l | tr -d ' '; }

# ============================================================================
# AGENT ROUTING
# ============================================================================

resolve_agent() {
    local phase="$1"
    local story_file="${2:-}"
    [[ -n "$AGENT_OVERRIDE" ]] && { echo "$AGENT_OVERRIDE"; return; }
    if [[ "$phase" == "dev" && -n "$story_file" && -f "$story_file" ]]; then
        local hint; hint=$(get_story_field "$story_file" "agent_hint" 2>/dev/null || echo "")
        [[ -n "$hint" ]] && { echo "$hint"; return; }
    fi
    case "$phase" in
        plan) echo "$PLAN_AGENT" ;;
        dev)  echo "$DEV_AGENT" ;;
        qa)   echo "$QA_AGENT" ;;
        *)    echo "claude" ;;
    esac
}

agent_model_flags() {
    local agent="$1" phase="$2"
    case "$agent" in
        claude)
            echo "--model sonnet"  # sonnet for all phases (plan, qa) — avoids opus limits
            ;;
        opencode)
            # opencode doesn't need model flags in this context
            echo "" ;;
        *) echo "" ;;
    esac
}

# ============================================================================
# AGENT INVOCATION
# ============================================================================

write_prompt() {
    mkdir -p "$(dirname "$PROMPT_TMP")"
    cat > "$PROMPT_TMP"
    echo "$PROMPT_TMP"
}

run_agent() {
    local prompt_file="$1"
    local phase="${2:-dev}"
    
    if $DRY_RUN; then
        info "[DRY RUN] $AGENT ($phase phase)"
        info "Prompt preview: $(head -3 "$prompt_file")..."
        return 0
    fi
    
    local model_flags
    model_flags=$(agent_model_flags "$AGENT" "$phase")
    info "  → $AGENT $model_flags (fresh context)"
    
    local rc=0
    case "$AGENT" in
        claude)  timeout "$STORY_TIMEOUT" claude --dangerously-skip-permissions $model_flags -p "$(cat "$prompt_file")" < /dev/null || rc=$? ;;
        codex)
            local tty_settings=""
            tty_settings=$(stty -g 2>/dev/null) || true
            timeout "$STORY_TIMEOUT" codex exec --full-auto "$(cat "$prompt_file")" < /dev/null || rc=$?
            [[ -n "$tty_settings" ]] && stty "$tty_settings" 2>/dev/null || true
            stty sane 2>/dev/null || true
            ;;
        gemini)  timeout "$STORY_TIMEOUT" gemini --yolo -p "$(cat "$prompt_file")" < /dev/null || rc=$? ;;
        opencode)  timeout "$STORY_TIMEOUT" opencode $model_flags -p "$(cat "$prompt_file")" < /dev/null || rc=$? ;;
        *)       err "Unknown agent: $AGENT"; exit 1 ;;
    esac
    
    if [[ $rc -eq 124 ]]; then
        warn "  Agent killed by timeout (${STORY_TIMEOUT}s)"
    elif [[ $rc -ne 0 ]]; then
        warn "  Agent exited with code $rc"
    fi
    return $rc
}

# ============================================================================
# PREFLIGHT: Auth + billing safety
# ============================================================================

preflight() {
    $DRY_RUN && return 0

    local agents=("$PLAN_AGENT" "$DEV_AGENT" "$QA_AGENT")
    $FROM_EXISTING && agents=("$DEV_AGENT" "$QA_AGENT")
    [[ -n "$AGENT_OVERRIDE" ]] && agents=("$AGENT_OVERRIDE")

    local unique_agents=() seen=""
    for a in "${agents[@]}"; do
        if [[ ! "$seen" == *"$a"* ]]; then
            unique_agents+=("$a")
            seen="$seen $a"
        fi
    done

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
        local rogue=false
        for a in "${unique_agents[@]}"; do
            case "$a" in
                claude)
                    [[ -n "${ANTHROPIC_API_KEY:-}" ]] && {
                        warn "ANTHROPIC_API_KEY set — Claude will bill API instead of subscription"
                        rogue=true
                    } ;;
                codex)
                    [[ -n "${OPENAI_API_KEY:-}" ]] && {
                        warn "OPENAI_API_KEY set — Codex will bill API instead of subscription"
                        rogue=true
                    } ;;
                gemini)
                    [[ -n "${GEMINI_API_KEY:-}${GOOGLE_API_KEY:-}" ]] && {
                        warn "GEMINI_API_KEY or GOOGLE_API_KEY set — Gemini will bill API instead of subscription"
                        rogue=true
                    } ;;
            esac
        done
        $rogue && warn "Unset rogue vars or run: uv run scripts/preflight_auth.py --fix"
    fi
}

# ============================================================================
# STEP 1: SM creates (or revises) a single story
# ============================================================================

sm_write_story() {
    local story_file="$1"
    local story_id; story_id=$(get_story_field "$story_file" "story_id")
    local title;    title=$(get_story_field "$story_file" "title")

    AGENT=$(resolve_agent "plan")
    phase "  ✏  SM: Writing/Revising story [$story_id] [$AGENT sonnet]"
    log_activity "SM_ITER" "$story_id" "SM_START" "SM writing/revising via $AGENT"

    local pf
    pf=$(write_prompt <<PROMPT_EOF
You are the Scrum Master. Your governing contract is @_bmad/orchestrator-master.md

You are working on ONE story for the iterative pipeline:
  Story ID: $story_id
  File:     @$story_file

Read:
@docs/prd.md
@docs/architecture.md
@docs/standards/scrum-master-guide.md

Also check:
@_bmad/progress.txt  (see what has already been completed in this MVP)
@_bmad/logs/activity.log  (look for past PO rejection notes for this story)

Your task (pick the correct one based on current story status):

A) If story status is "DRAFT" and content is mostly empty/template:
   → WRITE the full story. Decompose the PRD requirement this story covers.
   → Set: status: "DRAFT", po_alignment: "PENDING"
   → Include: Context, Requirements, Acceptance Criteria (numbered, testable),
     Implementation Notes, PO Alignment, QA Notes sections.
   → Set agent_hint:
       "codex"  → backend, API, database, infra, AND frontend
       "claude" → complex logic, data transforms, config
   → For UI/frontend stories: reference relevant docs/ui-mockups/ files.
   → Order dependencies clearly in Implementation Notes.

B) If story status is "REVISE":
   → READ the ## PO Alignment section — it contains the PO's revision notes.
   → Address EVERY issue raised.
   → Update story content to fix the issues.
   → Set: status: "DRAFT", po_alignment: "PENDING"
   → Append a dated note under ## PO Alignment: "SM revision: [summary of changes]"

Do NOT implement any code.
Do NOT approve the story yourself.
Do NOT touch any other story files.
Log a brief summary to _bmad/logs/activity.log.
PROMPT_EOF
    )

    run_agent "$pf" "plan" || true
    log_activity "SM_ITER" "$story_id" "SM_DONE" "Done"
}

# ============================================================================
# STEP 2: PO reviews a single story, returns 0=approved 1=needs-revision
# ============================================================================

po_review_story() {
    local story_file="$1"
    local story_id; story_id=$(get_story_field "$story_file" "story_id")

    AGENT=$(resolve_agent "plan")
    phase "  🔍 PO: Reviewing story [$story_id] [$AGENT sonnet]"
    log_activity "PO_ITER" "$story_id" "PO_START" "PO reviewing via $AGENT"

    local pf
    pf=$(write_prompt <<PROMPT_EOF
You are the Product Owner. Your governing contract is @_bmad/orchestrator-master.md

You are reviewing ONE story for the iterative pipeline:
  Story ID: $story_id
  File:     @$story_file

Read:
@docs/prd.md
@docs/architecture.md
@docs/standards/po-alignment-checklist.md

Also check:
@_bmad/progress.txt  (see what is already completed — this story must build on it correctly)

Evaluate this story against ALL of these criteria:
1. Maps to at least one PRD requirement (no orphan work)
2. Consistent with the architecture (correct layers, patterns, naming)
3. Requirements are clear, specific, and unambiguous
4. Acceptance Criteria are numbered, testable, and specific (not vague)
5. Scope is right-sized: completable in one focused dev effort
6. Dependencies are explicit: any assumed prior work exists or is listed
7. agent_hint is set correctly
8. No duplicate scope with other COMPLETED or READY_FOR_DEV stories

Decision — you MUST pick exactly one:

IF ALL criteria are met:
  → Set story frontmatter: status: "READY_FOR_DEV", po_alignment: "APPROVED"
  → Append under ## PO Alignment: "$(date -u +%Y-%m-%d) PO APPROVED: [brief rationale]"

IF ANY criterion fails:
  → Set story frontmatter: status: "REVISE", po_alignment: "REVISE"
  → Append under ## PO Alignment: "$(date -u +%Y-%m-%d) PO REVISE: [numbered list of specific issues]"

Log your decision to _bmad/logs/activity.log.
Do NOT implement code.
Do NOT move the story to IN_DEV or PENDING_QA.
Do NOT modify any other story files.
PROMPT_EOF
    )

    run_agent "$pf" "plan" || true

    local status; status=$(get_story_field "$story_file" "status")
    if [[ "$status" == "READY_FOR_DEV" ]]; then
        ok "  PO APPROVED [$story_id]"
        log_activity "PO_ITER" "$story_id" "PO_APPROVED" "Story approved"
        return 0
    else
        warn "  PO REVISE [$story_id] (status=$status) → looping back to SM"
        log_activity "PO_ITER" "$story_id" "PO_REVISE" "Needs SM revision"
        return 1
    fi
}

# ============================================================================
# SM ↔ PO LOOP: Runs for a single story until PO approves or max iters hit
# ============================================================================

run_smpo_loop() {
    local story_file="$1"
    local story_id; story_id=$(get_story_field "$story_file" "story_id")

    echo ""
    echo -e "${CYAN}  ┌─ SM↔PO Loop: $story_id ─────────────────────────────${NC}"

    local iter=0
    while [[ $iter -lt $MAX_SM_ITER ]]; do
        ((iter++)) || true
        echo -e "${CYAN}  │  Pass $iter/$MAX_SM_ITER${NC}"

        # SM writes/revises
        sm_write_story "$story_file"

        # PO reviews — returns 0 if approved
        if $SKIP_PO; then
            warn "  Skipping PO gate (--skip-po). Auto-approving."
            update_story_field "$story_file" "status" "READY_FOR_DEV"
            update_story_field "$story_file" "po_alignment" "APPROVED"
            log_activity "PO_ITER" "$story_id" "PO_SKIP" "Auto-approved"
            break
        fi

        if po_review_story "$story_file"; then
            echo -e "${CYAN}  └─ SM↔PO approved in $iter pass(es)${NC}"
            log_progress "$story_id: SM↔PO approved in $iter pass(es)"
            return 0
        fi

        # PO rejected — next iteration will have SM address the notes
        log_progress "$story_id: SM↔PO pass $iter — PO rejected, SM to revise"
    done

    # Check final status
    local final_status; final_status=$(get_story_field "$story_file" "status")
    if [[ "$final_status" == "READY_FOR_DEV" ]]; then
        echo -e "${CYAN}  └─ SM↔PO approved${NC}"
        return 0
    fi

    err "  SM↔PO loop stalled after $MAX_SM_ITER passes for $story_id (status=$final_status)"
    log_activity "SMPO" "$story_id" "SMPO_STALLED" "Max $MAX_SM_ITER passes"
    log_progress "$story_id: SM↔PO STALLED after $MAX_SM_ITER passes"
    return 1
}

# ============================================================================
# STEP 3: Dev implements a single story
# ============================================================================

dev_implement_story() {
    local story_file="$1"
    local story_id; story_id=$(get_story_field "$story_file" "story_id")
    local title;    title=$(get_story_field "$story_file" "title")

    AGENT=$(resolve_agent "dev" "$story_file")
    local hint; hint=$(get_story_field "$story_file" "agent_hint" 2>/dev/null || echo "default")
    phase "  💻 Dev: Implementing [$story_id] [$AGENT / $hint]"

    update_story_field "$story_file" "status" "IN_DEV"
    log_activity "DEV_ITER" "$story_id" "DEV_START" "Dev via $AGENT"

    local pf
    pf=$(write_prompt <<PROMPT_EOF
You are the Developer. Your governing contract is @_bmad/orchestrator-master.md

Working on ONE story — the CURRENT story in the iterative pipeline:
  ID:   $story_id
  File: @$story_file

Context:
@docs/architecture.md
@docs/prd.md
@_bmad/progress.txt

Run: \`git log --oneline -20\` to understand what the previous completed stories built.
This is an INCREMENTAL build. Each story adds to the working MVP. Build on what exists.

Design reference (for frontend stories):
If src/scaffolding/ exists, read BEFORE writing any UI code:
  - src/scaffolding/tokens.md    — design tokens (colors, fonts, spacing)
  - src/scaffolding/layouts/     — page layout templates
  - src/scaffolding/components/  — reusable UI component templates
Match the design language exactly. Do NOT invent new styles.

Task:
1. Read the story's Requirements and Acceptance Criteria carefully.
2. Write FAILING unit tests FIRST for each acceptance criterion (TDD).
3. Implement under src/ following architecture.md until all tests pass.
   For frontend: reference src/scaffolding/ templates; build in src/app/ or src/pages/.
4. Run ALL feedback loops — fix before declaring done:
   - Build: run the project build command
   - Test:  run the project test command
   - Lint:  run the project lint command
5. When build/test/lint PASS and ALL acceptance criteria are met:
   - Update story frontmatter: status: "PENDING_QA"
   - Fill in ## Implementation Notes: files changed, approach, key decisions
6. Append to _bmad/progress.txt:
   - What you built, files modified, decisions made, notes for QA
7. Commit: \`git add -A && git commit -m "feat($story_id): <summary>"\`

Rules:
- ONLY work on $story_id. Do NOT touch other stories.
- Do NOT skip feedback loops.
- If you cannot finish, commit partial progress, update progress.txt,
  and leave status "IN_DEV". The next dev iteration will continue.
PROMPT_EOF
    )

    run_agent "$pf" "dev" || true

    local st; st=$(get_story_field "$story_file" "status")
    if [[ "$st" == "PENDING_QA" ]]; then
        ok "  Dev done [$story_id] → PENDING_QA"
        log_activity "DEV_ITER" "$story_id" "DEV_DONE" "PENDING_QA"
        return 0
    else
        info "  Dev still IN_DEV [$story_id] (status=$st)"
        log_activity "DEV_ITER" "$story_id" "DEV_CONTINUE" "Not yet PENDING_QA"
        return 1
    fi
}

# ============================================================================
# STEP 4: QA reviews a single story, returns 0=pass 1=fail
# ============================================================================

qa_review_story() {
    local story_file="$1"
    local story_id; story_id=$(get_story_field "$story_file" "story_id")
    local title;    title=$(get_story_field "$story_file" "title")

    AGENT=$(resolve_agent "qa" "$story_file")
    phase "  🔬 QA: Reviewing [$story_id] [$AGENT opus]"
    log_activity "QA_ITER" "$story_id" "QA_START" "QA via $AGENT opus"

    local pf
    pf=$(write_prompt <<PROMPT_EOF
You are the QA Auditor. Your governing contract is @_bmad/orchestrator-master.md

Auditing ONE story — the CURRENT story in the iterative pipeline:
  ID:   $story_id
  File: @$story_file

Read:
@docs/prd.md
@docs/architecture.md
@docs/standards/qa-standards.md

Task:
1. Read Requirements, Acceptance Criteria, and Implementation Notes in the story.
2. Review the code files listed in Implementation Notes.
3. Run the test suite.
4. Verify EACH acceptance criterion against the actual implementation.
5. Check for regressions against prior completed stories (see _bmad/progress.txt).
6. Check code quality: no obvious bugs, no dead code, follows architecture patterns.

If ALL checks PASS:
  → Update story: qa_status: "PASS", status: "COMPLETED"
  → Append under ## QA Notes: what you tested, how, pass confidence, residual risks
  → Do NOT run git commit (orchestrator handles that)

If ANY check FAILS:
  → Update story: qa_status: "FAIL", status: "REFIX"
  → Append under ## QA Notes:
    - WHAT failed (specific, numbered list)
    - STEPS to reproduce each failure
    - SPECIFIC fix guidance for the developer
  → Do NOT commit

Log your decision to _bmad/logs/activity.log.
Do NOT modify any other story files.
PROMPT_EOF
    )

    run_agent "$pf" "qa" || true

    local ns; ns=$(get_story_field "$story_file" "status")
    if [[ "$ns" == "COMPLETED" ]]; then
        ok "  QA PASS [$story_id]"
        log_activity "QA_ITER" "$story_id" "QA_PASS" "Completed"
        return 0
    elif [[ "$ns" == "REFIX" ]]; then
        warn "  QA FAIL [$story_id] → looping back to Dev"
        log_activity "QA_ITER" "$story_id" "QA_FAIL" "Needs refix"
        return 1
    else
        warn "  QA ambiguous [$story_id] (status=$ns). Forcing REFIX."
        update_story_field "$story_file" "status" "REFIX"
        update_story_field "$story_file" "qa_status" "FAIL"
        log_activity "QA_ITER" "$story_id" "QA_FORCED_REFIX" "Ambiguous result"
        return 1
    fi
}

# ============================================================================
# DEV ↔ QA LOOP: Runs for a single story until QA passes or max iters hit
# ============================================================================

run_devqa_loop() {
    local story_file="$1"
    local story_id; story_id=$(get_story_field "$story_file" "story_id")
    local title;    title=$(get_story_field "$story_file" "title")

    echo ""
    echo -e "${CYAN}  ┌─ Dev↔QA Loop: $story_id ─────────────────────────────${NC}"

    local dev_iter=0
    while [[ $dev_iter -lt $MAX_DEV_ITER ]]; do

        # --- Dev phase ---
        local inner_dev=0
        while [[ $inner_dev -lt $MAX_DEV_ITER ]]; do
            ((inner_dev++)) || true
            echo -e "${CYAN}  │  Dev pass $inner_dev/$MAX_DEV_ITER${NC}"
            if dev_implement_story "$story_file"; then
                break  # Story is PENDING_QA
            fi
            local st; st=$(get_story_field "$story_file" "status")
            if [[ "$st" == "PENDING_QA" ]]; then break; fi
        done

        local current_status; current_status=$(get_story_field "$story_file" "status")
        if [[ "$current_status" != "PENDING_QA" ]]; then
            err "  Dev stalled [$story_id] after $inner_dev passes (status=$current_status)"
            log_activity "DEV_ITER" "$story_id" "DEV_STALLED" "Max passes"
            log_progress "$story_id: Dev STALLED"
            return 1
        fi

        ((dev_iter++)) || true

        # --- QA phase ---
        echo -e "${CYAN}  │  QA review (Dev↔QA cycle $dev_iter)${NC}"
        if qa_review_story "$story_file"; then
            echo -e "${CYAN}  └─ Dev↔QA passed in $dev_iter cycle(s)${NC}"
            log_progress "$story_id: Dev↔QA passed in $dev_iter cycle(s)"
            return 0
        fi

        # QA failed → back to dev (status is now REFIX)
        log_progress "$story_id: Dev↔QA cycle $dev_iter — QA failed, Dev to refix"

        if [[ $dev_iter -ge $MAX_DEV_ITER ]]; then break; fi
    done

    err "  Dev↔QA loop stalled after $MAX_DEV_ITER cycles for $story_id"
    log_activity "DEVQA" "$story_id" "DEVQA_STALLED" "Max $MAX_DEV_ITER cycles"
    log_progress "$story_id: Dev↔QA STALLED after $MAX_DEV_ITER cycles"
    return 1
}

# ============================================================================
# GIT COMMIT after each story passes QA
# ============================================================================

commit_story() {
    local story_file="$1"
    local story_id; story_id=$(get_story_field "$story_file" "story_id")
    local title;    title=$(get_story_field "$story_file" "title")

    if $NO_COMMIT || $DRY_RUN; then
        info "  Skipping git commit (--no-commit or --dry-run)"
        return
    fi

    git add -A 2>/dev/null || true
    if git commit -m "story($story_id): $title [QA PASS]" 2>/dev/null; then
        ok "  Committed story $story_id"
        log_activity "GIT" "$story_id" "COMMIT" "story($story_id): QA PASS"
        if git push 2>/dev/null; then
            log_activity "GIT" "$story_id" "PUSH" "ok"
        else
            warn "  git push failed (may need manual push)"
            log_activity "GIT" "$story_id" "PUSH_FAIL" "push failed"
        fi
    else
        warn "  git commit failed or nothing new to commit"
    fi
}

# ============================================================================
# DISCOVERY: What stories need to be processed?
# ============================================================================

discover_stories() {
    # Returns a sorted list of story files to process (in dependency/numeric order)
    local -a result=()

    if $FROM_EXISTING; then
        # Only process stories already in READY_FOR_DEV or REFIX
        while IFS= read -r l; do [[ -n "$l" ]] && result+=("$l"); done < <(
            { get_stories_by_status "READY_FOR_DEV"; get_stories_by_status "REFIX"; } | sort
        )
    else
        # Process ALL stories: DRAFT → SM/PO, REVISE → SM/PO, READY_FOR_DEV → Dev/QA, REFIX → Dev/QA
        while IFS= read -r l; do [[ -n "$l" ]] && result+=("$l"); done < <(
            {
                get_stories_by_status "DRAFT"
                get_stories_by_status "REVISE"
                get_stories_by_status "READY_FOR_DEV"
                get_stories_by_status "REFIX"
            } | sort
        )
    fi

    # Apply --start-from filter
    if [[ -n "$START_FROM" ]]; then
        local -a filtered=()
        local reached=false
        for f in "${result[@]}"; do
            local sid; sid=$(get_story_field "$f" "story_id")
            [[ "$sid" == "$START_FROM" ]] && reached=true
            $reached && filtered+=("$f")
        done
        result=("${filtered[@]}")
    fi

    printf '%s\n' "${result[@]}"
}

# ============================================================================
# THE MAIN ITERATIVE PIPELINE
# ============================================================================

run_iterative() {
    echo ""
    echo -e "${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║  BMADder Iterative Pipeline                               ║${NC}"
    echo -e "${CYAN}║  Story-by-Story: SM↔PO approval → Dev↔QA approval        ║${NC}"
    echo -e "${CYAN}║  Each story = a working, deployable MVP increment         ║${NC}"
    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"
    echo ""

    [[ -f "$ROOT/docs/prd.md" ]]         || { err "docs/prd.md missing.";         exit 1; }
    [[ -f "$ROOT/docs/architecture.md" ]] || { err "docs/architecture.md missing."; exit 1; }

    # ── Auto-plan bootstrap ────────────────────────────────────────────────
    # If no story files exist at all, invoke bmadder.sh plan --skip-po so the
    # SM agent creates DRAFT stubs from the PRD. The iterative SM↔PO loop
    # below then fleshes each story out one at a time.
    if ! $SKIP_PLAN && ! $FROM_EXISTING; then
        local story_count
        story_count=$(find "$STORIES_DIR" -name 'story-*.md' 2>/dev/null | wc -l | tr -d ' ')
        if [[ "$story_count" -eq 0 ]]; then
            info "No story files found. Running SM plan to create stubs..."
            local plan_script="$ROOT/scripts/bmadder.sh"
            [[ -f "$plan_script" ]] || { err "scripts/bmadder.sh not found."; exit 1; }
            local plan_flags="plan --skip-po"
            $DRY_RUN   && plan_flags="$plan_flags --dry-run"
            $SKIP_PO   && plan_flags="$plan_flags --skip-po"  # already set, harmless
            [[ -n "$AGENT_OVERRIDE" ]] && plan_flags="$plan_flags --agent $AGENT_OVERRIDE"
            bash "$plan_script" $plan_flags
            info "SM plan complete. Continuing iterative pipeline..."
        fi
    fi

    # Clean worktree before pipeline
    if ! git diff --quiet HEAD 2>/dev/null || [ -n "$(git ls-files --others --exclude-standard)" ]; then
        info "Committing uncommitted files before pipeline..."
        git add -A && git commit -m "chore: pre-iterative worktree snapshot" || true
    fi

    local -a stories=()
    while IFS= read -r l; do [[ -n "$l" ]] && stories+=("$l"); done < <(discover_stories)

    if [[ ${#stories[@]} -eq 0 ]]; then
        warn "No stories found to process."
        info "Tip: Create DRAFT stories in $STORIES_DIR, or use --from-existing"
        info "     to process READY_FOR_DEV/REFIX stories."
        return 0
    fi

    info "${#stories[@]} stories queued for iterative pipeline."
    log_activity "ORCH" "-" "ITERATIVE_START" "${#stories[@]} stories"

    local completed=0 skipped=0 stalled=0
    local story_num=0 total=${#stories[@]}

    for story_file in "${stories[@]}"; do
        [[ -f "$story_file" ]] || continue
        ((story_num++)) || true

        local story_id title current_status
        story_id=$(get_story_field "$story_file" "story_id")
        title=$(get_story_field "$story_file" "title")
        current_status=$(get_story_field "$story_file" "status")

        story_banner "STORY $story_num/$total — $story_id: $title"
        info "Current status: $current_status"

        # ── Phase 1: SM↔PO approval loop ───────────────────────────────────
        if [[ "$current_status" == "DRAFT" || "$current_status" == "REVISE" ]]; then
            phase "Phase 1 of 2: SM↔PO Story Approval"

            if ! run_smpo_loop "$story_file"; then
                warn "SM↔PO loop failed for $story_id — skipping to next story"
                log_activity "ORCH" "$story_id" "STORY_SKIP" "SM↔PO stalled"
                ((stalled++)) || true
                continue
            fi

            # Refresh status after SM↔PO
            current_status=$(get_story_field "$story_file" "status")
        fi

        # ── Phase 2: Dev↔QA implementation loop ────────────────────────────
        if [[ "$current_status" == "READY_FOR_DEV" || "$current_status" == "REFIX" ]]; then
            phase "Phase 2 of 2: Dev↔QA Implementation"

            if ! run_devqa_loop "$story_file"; then
                warn "Dev↔QA loop failed for $story_id — skipping to next story"
                log_activity "ORCH" "$story_id" "STORY_SKIP" "Dev↔QA stalled"
                ((stalled++)) || true
                continue
            fi

            # Story is now COMPLETED — commit it
            commit_story "$story_file"
            ((completed++)) || true

            echo ""
            ok "✅ Story $story_id COMPLETE — MVP has a new working increment!"
            log_activity "ORCH" "$story_id" "STORY_COMPLETE" "Committed"
            log_progress "$story_id: COMPLETE"

        elif [[ "$current_status" == "COMPLETED" ]]; then
            info "  Already COMPLETED — skipping."
            ((skipped++)) || true

        elif [[ "$current_status" == "IN_DEV" || "$current_status" == "PENDING_QA" ]]; then
            # Story is mid-flight — pick up where we left off
            warn "  Story is mid-flight (status=$current_status) — resuming Dev↔QA loop"
            if ! run_devqa_loop "$story_file"; then
                warn "Dev↔QA loop failed for $story_id — skipping to next story"
                ((stalled++)) || true
                continue
            fi
            commit_story "$story_file"
            ((completed++)) || true
            ok "✅ Story $story_id COMPLETE"
            log_progress "$story_id: COMPLETE (resumed)"
        else
            warn "  Unexpected status '$current_status' for $story_id — skipping."
            ((skipped++)) || true
        fi
    done

    # ── Final Report ──────────────────────────────────────────────────────
    echo ""
    echo -e "${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║  Iterative Pipeline Complete                              ║${NC}"
    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "  ${GREEN}Completed this run:${NC}  $completed"
    echo -e "  ${YELLOW}Already done:${NC}        $skipped"
    echo -e "  ${RED}Stalled:${NC}             $stalled"
    echo ""

    # Overall story counts
    local total_done; total_done=$(get_stories_by_status "COMPLETED" | wc -l | tr -d ' ')
    local total_all=0
    for s in DRAFT REVISE READY_FOR_DEV IN_DEV PENDING_QA REFIX COMPLETED; do
        local c; c=$(get_stories_by_status "$s" | wc -l | tr -d ' ')
        ((total_all += c)) || true
    done

    if [[ $total_done -eq $total_all && $total_all -gt 0 ]]; then
        echo -e "${GREEN}  🎉 ALL $total_all STORIES COMPLETED — MVP is fully built!${NC}"
        log_activity "ORCH" "-" "ITERATIVE_DONE" "All $total_all completed"
    else
        echo -e "${YELLOW}  $total_done / $total_all stories total have COMPLETED status${NC}"
        [[ $stalled -gt 0 ]] && warn "  $stalled stories stalled — review logs/activity.log"
        log_activity "ORCH" "-" "ITERATIVE_PARTIAL" "$total_done/$total_all"
    fi
    echo ""
}

# ============================================================================
# MAIN
# ============================================================================

main() {
    [[ -f "$BMAD_DIR/orchestrator-master.md" ]] || {
        err "Not a BMADder project. Run: uv run scripts/bootstrap_bmadder.py"
        exit 1
    }

    preflight
    run_iterative
}

main
