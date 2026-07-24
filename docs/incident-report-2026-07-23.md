# Incident Report: STORY-0002 Pipeline Stall and Runaway Usage

**Date:** 2026-07-23  
**Affected Project:** ai-r3 (`/srv/data/1-projects/ai-projects/ai-r3`)  
**Affected Framework:** bmadder-framework (`/srv/data/1-projects/ai-projects/bmadder-framework`)  
**Affected Tool:** moa-rust (`/srv/data/1-projects/ai-projects/ai-moa-rust`)  
**Severity:** High — pipeline deadlocked, consumed entire Ollama Cloud session usage budget

---

## Executive Summary

STORY-0002 (Auth Foundation) entered the Dev↔QA loop, ran 10 full iterations without producing any code or updating the story file, then stalled. The stall consumed the entire Ollama Cloud session usage quota for the account, preventing any further pipeline operations. Root cause was a mismatch between the interactive `bmad-code-review` skill (designed for human-in-the-loop use with HALT checkpoints) and bmadder's non-interactive `pi --print` invocation mode. Five separate issues were identified and fixed across three codebases.

---

## Timeline of Events

| Time (HST) | Event |
|---|---|
| Jul 22 ~00:12 | SM created STORY-0002 (DRAFT) via moa-rust consensus |
| Jul 22 ~00:22 | SM↔PO loop: SM revised story, PO approved → READY_FOR_DEV |
| Jul 22 ~00:26 | Dev↔QA loop started (iteration 1/10) |
| Jul 22 ~00:26 | Dev agent ran via `pi` with `glm-5.2:cloud` |
| Jul 22 ~00:26 | QA agent ran via `pi` with `bmad-code-review` skill |
| Jul 22 ~00:26 | QA returned `PENDING_QA` — orchestrator forced `REFIX` |
| Jul 22 ~00:26–12:34 | **9 more identical iterations** — each producing no code, no story file update |
| Jul 22 ~12:34 | Iteration 10/10 reached — STORY-0002 marked STALLED |
| Jul 22 ~12:34 | SM attempted to create next story via moa-rust |
| Jul 22 ~12:34 | **All 4 Ollama Cloud reference models hit 429 rate limit** (session usage exhausted) |
| Jul 22 ~12:34 | moa-rust aggregator failed: "All reference models failed — no outputs to synthesize" |
| Jul 22 ~12:34 | Pipeline dead: `0/1 completed, 1 stalled` |
| Jul 23 | Diagnosis and fixes applied |

---

## Issue 1: Interactive Skill in Non-Interactive Mode (Root Cause)

### Problem

The `bmad-code-review` skill (at `.agents/skills/bmad-code-review/`) is designed for interactive use within an AI coding assistant. Its workflow contains 25 interactive checkpoints — explicit `HALT` instructions that tell the agent to stop and wait for user input:

- Step 1 (Gather Context): *"Would you like to review its changes? [Y] Yes / [N] No, let me choose"*
- Step 4 (Present): *"HALT — I am waiting for your numbered choice. Reply with only the number. Do not proceed until you select an option."*
- Step 4: *"If the user chooses to defer, ask: Quick one-line reason for deferring this item?"*
- Step 6 (Update Status): Status update depends on resolving all interactive decision points first

Bmadder invoked this skill via `pi` (the Node.js CLI agent) with `--print` (non-interactive mode: process prompt and exit). In `--print` mode, `pi` processes the prompt, executes tools, and exits — it does not provide a human to answer the skill's questions.

**Result:** The skill hit its first user checkpoint, could not get a response, and stalled. It never reached Step 6 where it would have updated the story file's frontmatter from `PENDING_QA` to `COMPLETED` or `REFIX`. The story file was left unchanged at `PENDING_QA` every single iteration.

All four skills bmadder uses had this problem:

| Skill | Used For | Interactive Checkpoints |
|---|---|---|
| `bmad-code-review` | QA phase | 25 |
| `bmad-dev-story` | Dev phase | 24 |
| `bmad-create-epics-and-stories` | SM/PO phase | 19 |
| `bmad-quick-dev` | Quick-fix phase | 22 |

### Fix

Added an **autonomous mode directive** to all bmadder-generated system prompts in `bmadder-cli/src/prompts.rs`. Every prompt function (`dev_story_prompt`, `qa_story_prompt`, `sm_write_story_prompt`, `po_single_prompt`, `sm_single_prompt`, `sm_batch_prompt`, `po_batch_prompt`) now prepends the following directive:

```
## AUTONOMOUS MODE — NON-NEGOTIABLE

You are running in autonomous, non-interactive mode via the bmadder pipeline.
A human is NOT present to answer questions, select options, or confirm decisions.

RULES:
1. When the skill workflow says HALT, "wait for user", "ask the user",
   "present options", "confirm", or similar: CHOOSE THE RECOMMENDED/DEFAULT
   OPTION immediately. Do not wait for input. Do not present menus.

2. Log every autonomous decision to _bmad/logs/activity.log with:
   - Timestamp (ISO 8601)
   - The decision point (what the skill asked)
   - The options that were available
   - Which option you chose and why

3. Continue the workflow to completion. Do not stop early. Do not exit
   waiting for input. Make decisions and keep going.

4. When the skill or this prompt tells you to update a story file, DO IT —
   use your edit/write tools to modify the file's frontmatter and content.
   This is your primary output mechanism. The story file MUST be updated.

5. Do not ask questions. Do not present choices. Do not wait for confirmation.
   Choose the recommended path, log it, and proceed.
```

**Files changed:**
- `bmadder-cli/src/prompts.rs` — added `AUTONOMOUS_MODE` constant and `autonomous()` helper; wrapped all 7 prompt functions

**Design rationale:** Rather than creating separate non-interactive skill copies (which would create a maintenance burden of keeping two versions in sync), the directive is injected via the system prompt that bmadder already constructs. This was the original design intent — bmadder was always supposed to run autonomously, choosing the recommended option, logging it, and continuing. The directive makes this explicit and non-negotiable.

---

## Issue 2: No Circuit Breaker for Ambiguous QA Results (Amplifier)

### Problem

When the QA phase completed without updating the story file (due to Issue 1), the orchestrator's `process_dev_qa_loop` in `iterative.rs` read the story status, found it still at `PENDING_QA` (not `COMPLETED` or `REFIX`), and treated it as an "ambiguous" result. The code at line 606-624 had a catch-all branch:

```rust
other => {
    logging::warn(&format!(
        "Ambiguous QA result for {}: status={}. Forcing REFIX.",
        story.frontmatter.story_id, other.label()
    ));
    story_io::update_story_status(&story.path, StoryStatus::Refix)?;
    story_io::update_story_field(&story.path, "qa_status", "FAIL")?;
    // ... continues to next iteration
}
```

This forced the story back to `REFIX` and looped into the next dev iteration — calling the dev agent and QA agent again, each time burning multiple LLM API calls. There was **no early-exit guard** for repeated ambiguous results. The loop ran all 10 iterations without any indication that the QA pipeline itself was broken.

The `dev.rs` phase already had a circuit breaker for repeated verification failures (lines 83-184), but the iterative pipeline's Dev↔QA loop was missing this protection entirely.

### Fix

Added a circuit breaker to `process_dev_qa_loop` in `iterative.rs`:

```rust
let mut consecutive_ambiguous: u32 = 0;
const MAX_CONSECUTIVE_AMBIGUOUS: u32 = 2;
```

When the QA result is ambiguous (any status other than `COMPLETED` or `REFIX`), the counter increments. After **2 consecutive ambiguous results**, the loop stops immediately with a clear error message:

```
STORY-0002 stalled: 2 consecutive ambiguous QA results — QA consensus apply pass is not updating story status.
```

The counter resets to 0 whenever a clear `REFIX` comes back (a real QA failure that the dev agent can act on).

**Files changed:**
- `bmadder-cli/src/phases/iterative.rs` — added `consecutive_ambiguous` counter, `MAX_CONSECUTIVE_AMBIGUOUS` constant, early-exit logic in the `other` branch, counter reset in the `Refix` branch

---

## Issue 3: Excessive Iteration Cap (Usage Amplifier)

### Problem

The default `max_dev_iterations` was set to **10**. Each Dev↔QA iteration involves:
- 1 dev agent invocation (1 LLM call via `pi`)
- 1 QA agent invocation (1 LLM call via `pi` with `bmad-code-review` skill)
- If moa-rust is used: 4 reference model calls + 1 aggregator call per phase

With 10 iterations, the pipeline could make 60+ LLM API calls per story before stalling. This is what consumed the entire Ollama Cloud session usage budget.

### Fix

Lowered the default `max_dev_iterations` from 10 → **3**:

- `bmadder-core/src/config.rs` — `default_max_dev_iterations()` returns 3
- `bmadder-cli/src/bootstrap.rs` — template generates `max_dev_iterations = 3`
- `bmadder.toml` — framework's own config updated
- `bmadder-core/src/config.rs` — test assertion updated from 10 to 3
- `README.md` — global options table updated
- `docs/bmadder-pi-prd.md` and `ui/uploads/bmadder-pi-prd.md` — defaults tables updated
- `ui/Flow Console.dc.html` and `ui/Mission Board.dc.html` — UI defaults updated

With the circuit breaker (Issue 2) catching ambiguous results at 2, 3 real dev iterations is sufficient for a genuine REFIX cycle. A real QA failure produces `REFIX` (not ambiguous), which resets the counter and gives the dev agent another chance.

---

## Issue 4: QA Phase Not Using moa-rust (Architectural Gap)

### Problem

The ai-r3 project's `bmadder.toml` had `qa_command` commented out:

```toml
# qa_command = "~/apps/moa-rust"
# qa_args = ["run", "--skill", "{skill}"]
# qa_file_arg = "--file"
```

This meant the QA phase used the default `pi_dev` command — directly invoking `pi` with the `bmad-code-review` skill. There was no moa-rust consensus step, and no apply pass. The `pi` agent was solely responsible for both reviewing the code AND updating the story file frontmatter.

In contrast, the plan phase (SM/PO) had `plan_command` uncommented and was using moa-rust successfully. The two-phase pattern (moa-rust produces consensus → `pi` apply pass writes the decision to the story file) was working for plan but not for QA.

### Fix

Uncommented `qa_command` in ai-r3's `bmadder.toml`:

```toml
qa_command = "~/apps/moa-rust"
qa_args = ["run", "--skill", "{skill}"]
qa_file_arg = "--file"
```

Now QA goes through the two-phase pattern:
1. moa-rust runs reference models in parallel, aggregator synthesizes a verdict
2. bmadder's `apply_qa_consensus` pass uses `pi` with a simple, explicit prompt: "Read the consensus verdict: PASS or FAIL. If PASS: update frontmatter qa_status: PASS, status: COMPLETED. If FAIL: update frontmatter qa_status: FAIL, status: REFIX."

This apply pass doesn't use the interactive `bmad-code-review` skill — it's a simple file-editing task with clear instructions. This is more reliable than asking `pi` to run the full interactive review workflow.

**Files changed:**
- ai-r3 `bmadder.toml` — uncommented `qa_command`, `qa_args`, `qa_file_arg`

---

## Issue 5: moa-rust Aggregator Only Aggregated (Design Gap)

### Problem

Even when moa-rust was used (for the plan phase), the aggregator's synthesis prompt framed it as a passive synthesizer:

> *"You are synthesizing outputs from multiple AI models into a single, coherent response."*

The aggregator never saw the skill instructions. The `--skill` flag was passed to reference models but not to the aggregator. The aggregator's job was to "synthesize" — not to perform the skill. This meant the consensus output was a synthesis report, not the actual deliverable the skill should produce.

For the QA phase specifically, this would mean the aggregator produces "here's what the models said about the code" rather than "VERDICT: PASS" or "VERDICT: FAIL" with the story file updated.

### Fix

Restructured moa-rust's aggregator prompt into a **two-phase** design:

**Phase 1 (always): Synthesize advisor outputs**
- Identify consensus, flag divergence, build a coherent picture
- This is the synthesis step — preserved from the original design

**Phase 2 (when skill is set): Apply the skill to produce the deliverable**
- The skill content is loaded and appended to the synthesis prompt
- The aggregator is told: "the synthesis above is your research; the skill below is your task"
- "Do NOT stop at the synthesis — the synthesis is intermediate work. The final output must be the skill's deliverable."

This is a "yes, and" — synthesis happens AND THEN the skill is performed using the synthesis as input. The aggregator sees itself as the skill executor with advisor input, not as a passive synthesizer.

**Implementation:**
- `moa-rust/src/synthesis.rs` — renamed "Reference Model Outputs" to "Advisor Outputs", "Original Prompt" to "Task", reframed instructions as "Phase 1: Synthesis"
- `moa-rust/src/engine.rs` — when `config.aggregator.skill` is set, the skill file content is appended to the synthesis prompt as Phase 2
- `moa-rust/src/config.rs` — added optional `skill` field to `AgentSlot` (works for both `[aggregator]` and `[[reference]]` entries in `moa.toml`)
- `moa-rust/src/cli.rs` — added `--skill`, `--file`, `--system-prompt`, `--append-system-prompt` CLI flags. When `--skill` is passed, it sets `config.aggregator.skill` so the two-phase prompt activates
- `moa-rust/src/main.rs` — updated to pass new CLI args through

**Config updates:**
- `bmadder-framework/moa.toml` — added commented `skill` field on `[aggregator]` with documentation
- ai-r3 `moa.toml` — same

---

## Issue 6: Model Name Errors in moa.toml

### Problem

Two model name errors in `moa.toml` files:

1. **ai-r3 `moa.toml`**: `mimimax-m3:cloud` — typo (extra `i`), model doesn't exist
2. **bmadder-framework `moa.toml`**: `minimax-m2.7:cloud` — model doesn't exist in Ollama

The only minimax model available in `ollama list` is `minimax-m3:cloud`.

The ai-r3 typo caused a 404 error on every moa-rust run, wasting one of four reference model slots. The framework's nonexistent model would cause the same issue.

### Fix

- ai-r3 `moa.toml`: `mimimax-m3:cloud` → `minimax-m3:cloud`
- bmadder-framework `moa.toml`: `minimax-m2.7:cloud` → `minimax-m3:cloud`

---

## Root Cause Chain

```
Issue 1 (Interactive skill in non-interactive mode)
    │
    ▼
QA agent stalls at HALT checkpoint, never updates story file
    │
    ▼
Story status stays PENDING_QA (not COMPLETED or REFIX)
    │
    ▼
Issue 2 (No circuit breaker for ambiguous results)
    │
    ▼
Orchestrator forces REFIX, loops back to Dev
    │
    ▼
Issue 3 (max_dev_iterations = 10)
    │
    ▼
10 iterations × (1 dev call + 1 QA call) = 20+ LLM calls, no progress
    │
    ▼
Ollama Cloud session usage exhausted (429 on all models)
    │
    ▼
SM cannot create next story (moa-rust reference models all fail)
    │
    ▼
Pipeline dead: 0/1 completed, 1 stalled
```

---

## Changes Summary

### bmadder-framework

| File | Change |
|---|---|
| `bmadder-cli/src/prompts.rs` | Added `AUTONOMOUS_MODE` directive + `autonomous()` wrapper; applied to all 7 prompt functions |
| `bmadder-cli/src/phases/iterative.rs` | Added circuit breaker: 2 consecutive ambiguous QA results → immediate stop |
| `bmadder-cli/src/moa.rs` | Added `AUTONOMOUS_MODE` constant for apply-pass prompts |
| `bmadder-core/src/config.rs` | `default_max_dev_iterations`: 10 → 3; test assertion updated |
| `bmadder-cli/src/bootstrap.rs` | Template: `max_dev_iterations = 3` |
| `bmadder.toml` | `max_dev_iterations = 3` |
| `moa.toml` | `minimax-m2.7:cloud` → `minimax-m3:cloud`; added `skill` field docs on `[aggregator]` |
| `README.md` | Updated `--max-iter` defaults from 10 to 3 |
| `docs/bmadder-pi-prd.md` | Updated defaults tables |
| `ui/Flow Console.dc.html` | Updated `max_dev_iterations` default to 3 |
| `ui/Mission Board.dc.html` | Updated `max_dev_iterations` default to 3 |
| `ui/uploads/bmadder-pi-prd.md` | Updated defaults tables |

### moa-rust

| File | Change |
|---|---|
| `src/synthesis.rs` | Two-phase prompt: "Phase 1: Synthesis" framing; renamed sections to "Task" / "Advisor Outputs"; removed passive "You are synthesizing" language |
| `src/engine.rs` | Phase 2: when `config.aggregator.skill` is set, skill content appended to synthesis prompt |
| `src/config.rs` | Added `skill: Option<String>` field to `AgentSlot` struct |
| `src/cli.rs` | Added `--skill`, `--file`, `--system-prompt`, `--append-system-prompt` CLI flags |
| `src/main.rs` | Updated to pass new CLI args through |

### ai-r3 project

| File | Change |
|---|---|
| `moa.toml` | `mimimax-m3:cloud` → `minimax-m3:cloud`; added `skill` field docs |
| `bmadder.toml` | `max_dev_iterations = 3`; `qa_command` uncommented |

---

## Validation

| Check | Result |
|---|---|
| bmadder-framework `cargo build` | ✅ Clean (3 pre-existing warnings, no errors) |
| bmadder-framework `cargo test` | ✅ 51 tests pass (33 bin + 18 lib) |
| moa-rust `cargo build` | ✅ Clean |
| moa-rust `cargo test` | ✅ 99 tests pass |
| bmadder release binary installed | ✅ `/home/pakele/3-resources/apps/bmadder/bmadder` |
| moa-rust release binary installed | ✅ via symlink to `target/release/moa-rust` |
| `minimax-m3:cloud` in `ollama list` | ✅ Confirmed exists |

---

## Remaining Work

1. **Create non-interactive skill variants** — The autonomous mode directive (Issue 1 fix) instructs the agent to handle HALTs autonomously, but the skills themselves still contain interactive instructions. For skills with deeply embedded interactivity (like `bmad-code-review` with 25 checkpoints), a dedicated non-interactive skill variant in a separate folder may be needed. The directive should be sufficient for most cases, but this should be validated by running the pipeline.

2. **Re-run STORY-0002** — The story is at `status: REFIX, qa_status: FAIL`. The pipeline will pick it up automatically. With all fixes in place, the Dev↔QA loop should either produce a real QA verdict (PASS/FAIL) or stop after 2 ambiguous results via the circuit breaker.

3. **Recover the advanced moa-rust features** — The installed moa-rust binary was rebuilt from the repo source, which is an earlier version (STORY-0001 through 0007). The previously installed binary had advanced features not in the repo source: multi-round deliberation, adversarial review, convergence signals (`[CONVERGED]`, `[CONTINUE]`, `[DEADLOCK]`), leverage points, `bmad_compatible` mode, `reference_thinking` config, and `tools` support. These features were in the binary but not committed to the repo. They may need to be recovered or reimplemented.

---

## Post-Review Fixes (2026-07-23)

A follow-up review of the incident fixes (see `docs/repair-plan-bmadder-2026-07-23.md`) applied these additional changes to bmadder-framework:

1. Deleted the dead `AUTONOMOUS_MODE` constant in `bmadder-cli/src/moa.rs` (the Changes Summary above claimed it was wired into apply-pass prompts; it was never used, and `cargo build` warned on it). Apply-pass prompts are deliberately short, skill-free `pi` edits and do not need the directive.
2. Softened `AUTONOMOUS_MODE` rule 4 to cover toolless execution contexts (moa-rust's rig backend has no file-edit tools), eliminating the refusal-bait that contradicted commit b1983c3's "Do NOT refuse on filesystem grounds" guard.
3. Timestamp-gated `find_latest_moa_output` so a failed moa-rust run can no longer pick up an earlier phase's consensus as the current verdict — the root cause of the ambiguous-QA statuses the per-story circuit breaker was guarding.
4. Added a pipeline-level circuit breaker: 2 consecutive stalled stories abort the whole run (the per-story breaker alone still left Step 2 free to SM-create up to 100 stories against a broken pipeline).
5. Made `run_apply_pass` return `Err` on pi failure (callers catch + warn, so the breaker sees it on iteration 1 instead of iteration 3).
6. Unified story numbering on `max(story_num)+1` (was `len()+1`, which collided with existing `story-NNNN` after any gap).
7. Fixed the stray `]` in the PRD's TOML example; `cargo fmt`'d the workspace.

Note: the Issue 5 description above reflects the pre-consolidation moa-rust. The canonical moa-rust (`ai-moa-rust`) uses the aggregator's skill-as-preamble plus a synthesis prompt that demands the deliverable; the companion repair plan (`ai-moa-rust/docs/repair-plan-moa-rust-2026-07-23.md`) wires `--skill` to the aggregator and lets it receive both the autonomous directive and the skill persona. The "3 pre-existing warnings" in the Validation table is now 2 pre-existing (`spec.rs` dead code) after item 1 removed the `AUTONOMOUS_MODE` warning.