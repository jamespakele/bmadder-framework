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
#   --agent AGENT       Force ALL phases to use this agent
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
HEADLESS_DIR="$ROOT/scripts/headless-skills"
LOG_FILE="$BMAD_DIR/logs/activity.log"
PROGRESS_FILE="$BMAD_DIR/progress.txt"
PROMPT_TMP="$BMAD_DIR/.prompt-itmp.md"

# --- Defaults ---
AGENT_OVERRIDE="${BMADDER_AGENT:-}"
PLAN_AGENT="${BMADDER_PLAN_AGENT:-claude}"
DEV_AGENT="${BMADDER_DEV_AGENT:-codex}"
QA_AGENT="${BMADDER_QA_AGENT:-claude}"
MAX_SM_ITER="${BMADDER_MAX_SM_ITER:-5}"
MAX_DEV_ITER="${BMADDER_MAX_DEV_ITER:-10}"
STORY_TIMEOUT="${BMADDER_STORY_TIMEOUT:-1800}"
DRY_RUN=false
SKIP_PO=false
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
            # Pass prompt via stdin ("-" arg) — avoids shell ARG_MAX limits for long prompts.
            # --dangerously-bypass-approvals-and-sandbox: needed so codex can run build/test
            # commands (cargo build, cargo test, etc.) without approval prompts in CI-style use.
            timeout "$STORY_TIMEOUT" codex exec --dangerously-bypass-approvals-and-sandbox - < "$prompt_file" || rc=$?
            [[ -n "$tty_settings" ]] && stty "$tty_settings" 2>/dev/null || true
            stty sane 2>/dev/null || true
            ;;
        gemini)  timeout "$STORY_TIMEOUT" gemini --yolo -p "$(cat "$prompt_file")" < /dev/null || rc=$? ;;
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
You are the Scrum Master running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated headless skill instructions:
@scripts/headless-skills/sm-create-story.md

You are working on ONE story for the iterative pipeline:
  Story ID: $story_id
  File:     @$story_file

Context documents:
@docs/prd.md
@docs/architecture.md
@_bmad/progress.txt
@_bmad/logs/activity.log

Your task (pick the correct one based on current story status):

A) If story status is "DRAFT" and content is mostly empty/template:
   → WRITE the full story following the workflow and checklist.
   → Set: status: "DRAFT", po_alignment: "PENDING"

B) If story status is "REVISE":
   → READ the ## PO Alignment section for revision notes.
   → Address EVERY issue raised. Update story content.
   → Set: status: "DRAFT", po_alignment: "PENDING"
   → Append dated note under ## PO Alignment: "SM revision: [summary of changes]"

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
You are the Product Owner running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the story quality checklist:
@scripts/headless-skills/po-review.md

You are reviewing ONE story for the iterative pipeline:
  Story ID: $story_id
  File:     @$story_file

Context documents:
@docs/prd.md
@docs/architecture.md
@_bmad/progress.txt

Evaluate this story against the checklist criteria above PLUS:
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
You are the Developer running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated dev workflow:
@scripts/headless-skills/dev-story.md

Working on ONE story — the CURRENT story in the iterative pipeline:
  ID:   $story_id
  File: @$story_file

Context:
@docs/architecture.md
@docs/prd.md
@_bmad/progress.txt

Run: \`git log --oneline -20\` to understand what the previous completed stories built.
This is an INCREMENTAL build. Each story adds to the working MVP. Build on what exists.

Completion criteria:
- When build/test/lint PASS and ALL acceptance criteria are met:
  - Update story frontmatter: status: "PENDING_QA"
  - Fill in ## Implementation Notes: files changed, approach, key decisions
- Append to _bmad/progress.txt: what you built, files modified, decisions, notes for QA
- Commit: \`git add -A && git commit -m "feat($story_id): <summary>"\`

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
You are the QA Auditor running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated code review workflow:
@scripts/headless-skills/qa-review.md

Auditing ONE story — the CURRENT story in the iterative pipeline:
  ID:   $story_id
  File: @$story_file

Context:
@docs/prd.md
@docs/architecture.md
@_bmad/progress.txt

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
# DISCOVERY: In-flight stories to resume at pipeline start
# ============================================================================

discover_stories() {
    local -a result=()

    if $FROM_EXISTING; then
        while IFS= read -r l; do [[ -n "$l" ]] && result+=("$l"); done < <(
            { get_stories_by_status "READY_FOR_DEV"; get_stories_by_status "REFIX"; } | sort
        )
    else
        while IFS= read -r l; do [[ -n "$l" ]] && result+=("$l"); done < <(
            {
                get_stories_by_status "DRAFT"
                get_stories_by_status "REVISE"
                get_stories_by_status "READY_FOR_DEV"
                get_stories_by_status "REFIX"
                get_stories_by_status "IN_DEV"
                get_stories_by_status "PENDING_QA"
            } | sort
        )
    fi

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
# SM: Create the NEXT single story from the PRD
#
# Echoes the path to the newly created story file.
# Returns 0 if a story was created, 1 if PRD is fully implemented / stalled.
# ============================================================================

sm_create_next_story() {
    AGENT=$(resolve_agent "plan")
    mkdir -p "$STORIES_DIR"
    local before_files
    before_files=$(find "$STORIES_DIR" -name 'story-*.md' 2>/dev/null | sort)

    phase "  [SM] Creating next story [$AGENT sonnet]"
    log_activity "SM_NEXT" "-" "SM_NEXT_START" "Creating next story via $AGENT"

    local today; today=$(date +%Y-%m-%d)
    local pf prompt_body
    prompt_body="You are the Scrum Master running in AUTOMATED PIPELINE mode (non-interactive, no user input).

Follow the consolidated headless skill instructions:
@scripts/headless-skills/sm-create-story.md

Context documents:
@docs/prd.md
@docs/architecture.md
@_bmad/progress.txt

Also run: \`git log --oneline -30\`
And review existing stories in: docs/backlog/stories/

Your task -- pick exactly ONE:

A) If the PRD has features NOT yet implemented (no story file and not in progress.txt):
   -> Create ONE story file following the skill workflow and checklist.
   -> Respect dependencies: foundational/infrastructure stories first.
   -> Filename: docs/backlog/stories/story-NNNN-<slug>.md
      (NNNN = next available 4-digit number)
   -> Use the template from the skill for story structure.
   -> Frontmatter must include at minimum:
       story_id: \"STORY-NNNN\"
       title: \"<concise title>\"
       status: \"DRAFT\"
       po_alignment: \"PENDING\"
       created_at: \"$today\"
       updated_at: \"$today\"
   -> Log to _bmad/logs/activity.log: \"SM_NEXT: created STORY-NNNN -- <title>\"

B) If the PRD is FULLY implemented:
   -> Append this exact line to _bmad/progress.txt:
       \"ALL_DONE: PRD fully implemented.\"
   -> Do NOT create any story file.

Create ONLY ONE story file. Do not implement code."

    pf=$(write_prompt < <(echo "$prompt_body"))
    run_agent "$pf" "plan" || true

    # Detect newly created story file by diffing before/after
    local after_files new_story=""
    after_files=$(find "$STORIES_DIR" -name 'story-*.md' 2>/dev/null | sort)
    while IFS= read -r f; do
        if [[ -n "$f" ]] && ! echo "$before_files" | grep -qF "$f" 2>/dev/null; then
            new_story="$f"
            break
        fi
    done <<< "$after_files"

    if [[ -n "$new_story" && -f "$new_story" ]]; then
        local sid; sid=$(get_story_field "$new_story" "story_id")
        ok "  SM created: $sid -- $(get_story_field "$new_story" "title")"
        log_activity "SM_NEXT" "$sid" "SM_NEXT_DONE" "Story created"
        # Write path to result file — avoids stdout capture bug in command substitution
        echo "$new_story" > "$BMAD_DIR/.sm_next_result"
        return 0
    fi

    if grep -q "ALL_DONE" "$PROGRESS_FILE" 2>/dev/null; then
        ok "  SM: PRD fully implemented -- pipeline complete."
        log_activity "SM_NEXT" "-" "ALL_DONE" "PRD complete"
        rm -f "$BMAD_DIR/.sm_next_result"
        return 1
    fi

    warn "  SM did not create a story (stalled or PRD complete)"
    log_activity "SM_NEXT" "-" "SM_NEXT_NONE" "No story created"
    rm -f "$BMAD_DIR/.sm_next_result"
    return 1
}

# ============================================================================
# Process one story: SM/PO approval then Dev/QA implementation
# ============================================================================

process_one_story() {
    local story_file="$1"
    local story_id current_status
    story_id=$(get_story_field "$story_file" "story_id")
    current_status=$(get_story_field "$story_file" "status")

    info "Current status: $current_status"

    # Phase 1: SM/PO approval loop (for DRAFT or REVISE stories)
    if [[ "$current_status" == "DRAFT" || "$current_status" == "REVISE" ]]; then
        phase "Phase 1 of 2: SM/PO Story Approval"
        if ! run_smpo_loop "$story_file"; then
            warn "SM/PO loop failed for $story_id -- skipping"
            log_activity "ORCH" "$story_id" "STORY_SKIP" "SM/PO stalled"
            return 1
        fi
        current_status=$(get_story_field "$story_file" "status")
    fi

    # Phase 2: Dev/QA implementation loop
    if [[ "$current_status" == "READY_FOR_DEV" || "$current_status" == "REFIX" \
       || "$current_status" == "IN_DEV" || "$current_status" == "PENDING_QA" ]]; then
        phase "Phase 2 of 2: Dev/QA Implementation"
        if ! run_devqa_loop "$story_file"; then
            warn "Dev/QA loop failed for $story_id -- skipping"
            log_activity "ORCH" "$story_id" "STORY_SKIP" "Dev/QA stalled"
            return 1
        fi
        commit_story "$story_file"
        echo ""
        ok "STORY $story_id COMPLETE -- MVP has a new working increment!"
        log_activity "ORCH" "$story_id" "STORY_COMPLETE" "Committed"
        log_progress "$story_id: COMPLETE"
        return 0

    elif [[ "$current_status" == "COMPLETED" ]]; then
        info "  Already COMPLETED -- skipping."
        return 0

    else
        warn "  Unexpected status '$current_status' for $story_id -- skipping."
        return 1
    fi
}

# ============================================================================
# MAIN ITERATIVE PIPELINE
#
# True iterative flow per iteration:
#   SM creates next story -> PO approves -> Dev implements -> QA checks -> commit
#   Repeat until SM declares PRD fully implemented.
# ============================================================================

run_iterative() {
    echo ""
    echo -e "${CYAN}+-----------------------------------------------------------+${NC}"
    echo -e "${CYAN}|  BMADder Iterative Pipeline                               |${NC}"
    echo -e "${CYAN}|  SM creates -> PO approves -> Dev builds -> QA checks     |${NC}"
    echo -e "${CYAN}|  One story at a time. Each = a deployable MVP increment.  |${NC}"
    echo -e "${CYAN}+-----------------------------------------------------------+${NC}"
    echo ""

    [[ -f "$ROOT/docs/prd.md" ]]         || { err "docs/prd.md missing.";         exit 1; }
    [[ -f "$ROOT/docs/architecture.md" ]] || { err "docs/architecture.md missing."; exit 1; }

    # Clean worktree before pipeline starts
    if ! git diff --quiet HEAD 2>/dev/null || [ -n "$(git ls-files --others --exclude-standard)" ]; then
        info "Committing uncommitted files before pipeline..."
        git add -A && git commit -m "chore: pre-iterative worktree snapshot" || true
    fi

    local completed=0 stalled=0

    # Step 1: Resume any already-existing in-flight stories first
    local -a inflight=()
    while IFS= read -r l; do [[ -n "$l" ]] && inflight+=("$l"); done < <(discover_stories)

    if [[ ${#inflight[@]} -gt 0 ]]; then
        info "Resuming ${#inflight[@]} in-flight story/stories before starting new ones..."
        for story_file in "${inflight[@]}"; do
            [[ -f "$story_file" ]] || continue
            local sid; sid=$(get_story_field "$story_file" "story_id")
            local ttl; ttl=$(get_story_field "$story_file" "title")
            story_banner "RESUMING -- $sid: $ttl"
            if process_one_story "$story_file"; then
                ((completed++)) || true
            else
                ((stalled++)) || true
            fi
        done
    fi

    # Step 2: True iterative loop -- SM creates one story at a time
    if ! $FROM_EXISTING; then
        local max_stories=100
        local iterations=0
        log_activity "ORCH" "-" "ITERATIVE_START" "Entering SM-driven loop"

        while [[ $iterations -lt $max_stories ]]; do
            ((iterations++)) || true
            echo ""
            echo -e "${MAGENTA}-- Iteration $iterations -----------------------------------------------${NC}"

            # SM creates the next story — writes path to .sm_next_result (no subshell capture)
            rm -f "$BMAD_DIR/.sm_next_result"
            if ! sm_create_next_story; then
                ok "SM signals PRD is fully implemented. Pipeline complete."
                break
            fi

            local new_story=""
            [[ -f "$BMAD_DIR/.sm_next_result" ]] && new_story=$(cat "$BMAD_DIR/.sm_next_result")
            [[ -z "$new_story" || ! -f "$new_story" ]] && {
                warn "SM returned no story path -- stopping."
                break
            }

            local sid; sid=$(get_story_field "$new_story" "story_id")
            local ttl; ttl=$(get_story_field "$new_story" "title")
            story_banner "NEW STORY -- $sid: $ttl"

            if process_one_story "$new_story"; then
                ((completed++)) || true
            else
                ((stalled++)) || true
                warn "Story $sid stalled. Continuing to next story..."
            fi
        done

        [[ $iterations -ge $max_stories ]] && \
            warn "Safety limit ($max_stories stories) reached. Re-run to continue."
    fi

    # Final report
    echo ""
    echo -e "${CYAN}+-----------------------------------------------------------+${NC}"
    echo -e "${CYAN}|  Iterative Pipeline Complete                              |${NC}"
    echo -e "${CYAN}+-----------------------------------------------------------+${NC}"
    echo ""
    echo -e "  ${GREEN}Completed this run:${NC}  $completed"
    echo -e "  ${RED}Stalled:${NC}             $stalled"
    echo ""

    local total_done; total_done=$(get_stories_by_status "COMPLETED" | wc -l | tr -d ' ')
    if grep -q "ALL_DONE" "$PROGRESS_FILE" 2>/dev/null; then
        echo -e "${GREEN}  PRD FULLY IMPLEMENTED -- $total_done stories completed!${NC}"
        log_activity "ORCH" "-" "ITERATIVE_DONE" "All done"
    else
        echo -e "${YELLOW}  $total_done stories completed so far${NC}"
        [[ $stalled -gt 0 ]] && warn "  $stalled stories stalled -- review _bmad/logs/activity.log"
        log_activity "ORCH" "-" "ITERATIVE_PARTIAL" "$total_done completed"
    fi
    echo ""
}

# ============================================================================
# MAIN
# ============================================================================

main() {
    [[ -f "$BMAD_DIR/orchestrator-master.md" ]] || {
        err "Not a BMADder project. Run: python3 scripts/bootstrap_bmadder.py"
        exit 1
    }

    preflight
    run_iterative
}

main
