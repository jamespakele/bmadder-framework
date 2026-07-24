# Repair Plan: bmadder-framework

**Repo:** `/srv/data/1-projects/ai-projects/bmadder-framework`
**Context:** Follow-up to `docs/incident-report-2026-07-23.md` (STORY-0002 pipeline stall / quota exhaustion). The incident fixes are in the working tree (uncommitted) + recent commits. Review found gaps on the bmadder side: a dead constant the report claims is wired, a stale-consensus hazard that regenerates the exact ambiguous-QA failure the new circuit breaker guards, no pipeline-level breaker, swallowed apply-pass failures, a refusal-bait contradiction in the autonomous directive, and doc/format debt.

The canonical moa-rust (`/srv/data/1-projects/ai-projects/ai-moa-rust`) is repaired by a companion plan (`/srv/data/1-projects/ai-projects/ai-moa-rust/docs/repair-plan-moa-rust-2026-07-23.md`). Key facts that shape THIS plan:
- moa-rust's default backend is **rig** (in-process LLM), which has **no file-edit tools**. The pi-dev backend uses `--no-tools` unless `config.pi.tools` is set. So in the moa-rust path, reference + aggregator models are **toolless** — only bmadder's own apply passes (plain `pi`) have edit tools. This is why Task 2 (soften the autonomous directive) is required.
- moa-rust now parses all deployed `moa.toml` keys (`backend`, `reference_thinking`, `bmad_compatible`, `max_rounds`, `tools`, `api_key`). **There are no stale config keys to prune** — the earlier "prune moa.toml" idea is dropped.
- bmadder invokes moa-rust with `run --skill <dir> --system-prompt <text> --file <path>...` and **no positional prompt**; moa-rust reads stdin (bmadder sets stdin null) → empty prompt. This already works. No bmadder change needed for prompt routing.

**Do NOT** run project-wide formatters/linters except where a task says so. Run the verification block at the end only.

---

## Task 1 — Delete the dead `AUTONOMOUS_MODE` constant in `moa.rs` (build warning; report/code mismatch)

**Files:** `bmadder-cli/src/moa.rs` (~line 26).

`cargo build` warns `constant AUTONOMOUS_MODE is never used`. The incident report's Changes Summary claims it was "added for apply-pass prompts," but `run_apply_pass` (moa.rs ~line 400) never prepends it. The apply-pass prompts are deliberately short, explicit, skill-free file edits (incident Issue 4 says that is WHY they're reliable), and they run through plain `pi` which has edit tools — they do not hit HALT checkpoints.

**Change:** DELETE the constant from `moa.rs` (do not wire it in). It solves a problem the apply passes don't have, and it has already drifted from the canonical version in `prompts.rs`. If a future need arises, `prompts.rs::AUTONOMOUS_MODE` should be made `pub(crate)` and reused — never duplicated.

**Acceptance:** `cargo build` no longer emits the `AUTONOMOUS_MODE` warning. No behavior change.

## Task 2 — Harden AUTONOMOUS_MODE rule 4 for toolless execution contexts

**File:** `bmadder-cli/src/prompts.rs` (const `AUTONOMOUS_MODE`, ~line 13).

The directive is prepended to ALL prompt functions, including SM/PO/QA prompts that route through moa-rust when `plan_command`/`qa_command` are set. moa-rust's rig backend models have **no file-edit tools** (see context above), yet rule 4 commands "use your edit/write tools" unconditionally. This is refusal bait for exactly the toolless models that commit b1983c3 ("stop SM prompts from triggering toolless-model refusals") was fixing, and it contradicts the prompts' own "Do NOT refuse on the grounds that you cannot access the filesystem" line.

**Change:** reword rule 4 to cover both contexts, e.g.:

> 4. When the skill or this prompt tells you to update a story file: if you have edit/write tools, DO IT — modify the file's frontmatter and content directly. If you do NOT have file tools, emit the complete deliverable (the full updated story content / your verdict) as your response text — a downstream apply pass will write it. Never refuse or stall because you cannot access the filesystem.

Keep the other 4 rules unchanged. Update the doc comment above the constant to note the two execution contexts (direct pi = has tools; via moa-rust rig = toolless, apply pass writes).

**Acceptance:** existing prompt tests updated if they assert on the old rule-4 text; `cargo test` passes.

## Task 3 — Gate consensus pickup on invocation time (CRITICAL: stale-consensus hazard)

**Files:** `bmadder-cli/src/moa.rs` (`find_latest_moa_output`, ~line 34-61) and its 7 call sites: `phases/iterative.rs` (~325, ~381, ~557, ~766), `phases/plan.rs` (~102, ~199), `phases/qa.rs` (~87).

`find_latest_moa_output` returns the newest `output/moa-*.md` regardless of which invocation produced it. Failure mode: QA runs via moa-rust, moa-rust fails to write output (all references fail → moa-rust exits non-zero, no new file written), and the "latest" file is the SM consensus from earlier in the same run — `apply_qa_consensus` then asks pi to extract PASS/FAIL from a story-authoring document. That manufactures exactly the ambiguous statuses the new circuit breaker (iterative.rs ~line 426) fires on. Fix at the source, not palliate.

**Change:**
- New signature: `find_latest_moa_output(config: &Config, newer_than: std::time::SystemTime) -> Option<PathBuf>` — only return a file whose mtime is strictly newer than `newer_than`.
- At every call site, capture `let invoked_at = std::time::SystemTime::now();` immediately BEFORE the `invoke_agent_plan`/`invoke_agent_qa` call whose output is being sought, and pass it through.
- When no qualifying file exists, the existing "no consensus output found" warn branches fire (they already exist at every site) — that is the correct behavior; do not invent new fallbacks.

**Acceptance:**
- Unit test in `moa.rs`: temp `output/` dir with an old `moa-a.md`; `find_latest_moa_output(cfg, now)` → `None`; touch a new `moa-b.md` after `now` → returns `moa-b.md`.
- All 7 call sites pass a timestamp captured before their invocation. `cargo test` passes.

## Task 4 — Pipeline-level circuit breaker for consecutive stalled stories

**File:** `bmadder-cli/src/phases/iterative.rs` (`run_iterative`, ~line 59-198).

The new per-story breaker stops one story after 2 ambiguous QA results, but `run_iterative` counts a stall and keeps going: Step 2 will SM-create up to `max_iterations = 100` new stories, each burning SM consensus + dev + 2 ambiguous QA rounds against the same broken pipeline. The incident was a quota-exhaustion event; this is the remaining unbounded burn path.

**Change:** track `consecutive_stalled: u32` in `run_iterative` (shared across the in-flight loop and Step 2). Increment when `process_one_story` returns `Ok(false)` or `Err`; reset on `Ok(true)`. At `MAX_CONSECUTIVE_STALLED: u32 = 2`, log an err ("N consecutive stories stalled — aborting pipeline; see per-story STALLED entries"), `log_activity` an `ORCH / PIPELINE_ABORT` entry, and break out of the whole run (fall through to the existing final report, which already prints completed/stalled/total).

**Acceptance:** mandatory: the abort path logs BOTH to console and activity log, and the final report still prints. Prefer a small pure helper for the counter decision if it aids testing; otherwise document the manual reasoning in the commit message.

## Task 5 — Propagate apply-pass failure instead of swallowing it

**Files:** `bmadder-cli/src/moa.rs` (`run_apply_pass`, ~line 400-431) and the callers of `apply_qa_consensus`/`apply_po_consensus`/`apply_sm_consensus`/`*_batch` in `phases/iterative.rs`, `phases/plan.rs`, `phases/qa.rs`.

`run_apply_pass` logs `warn` and returns `Ok(())` when pi reports failure. The apply pass is now the single point where verdicts become story state; its failure currently surfaces two expensive iterations later as "ambiguous QA result".

**Change:** make `run_apply_pass` return `Err` when `!result.success` (include `result.error` in the message). At call sites, do NOT crash the pipeline: catch the error, log it, and treat it the same as "no consensus output found" (status unchanged → the ambiguity/breaker machinery sees it immediately on THIS iteration). Keep the `?` only where an error already meant abort; where the current code ignores the result, wrap in `if let Err(e) = ... { logging::warn(...) }`.

**Acceptance:** grep shows no call site silently discards an apply-pass failure; behavior on failure is warn + fall through to the existing unchanged-status handling. `cargo test` passes.

## Task 6 — Unify story numbering on max-existing-number

**File:** `bmadder-cli/src/moa.rs` (`format_consensus_as_story`, ~line 72-73).

`format_consensus_as_story` computes `next_num = existing.len() + 1`, while `format_consensus_into_stories` (~line 184) and the SM prompt listing (commit 12b4e68) use `max(story_num) + 1`. With any numbering gap (deleted/renamed story), `len()+1` collides with an existing `story-NNNN`.

**Change:** use the same `story_num`-max computation as `format_consensus_into_stories` (the helper `story_num` already exists at ~line 391).

**Acceptance:** unit test: stories dir containing `story-0001-a.md` and `story-0007-b.md` → next number is 8, not 3.

## Task 7 — Doc and hygiene fixes

1. `docs/bmadder-pi-prd.md:160` — stray `]` in `]max_dev_iterations       = 3` inside a TOML code block. Remove it. Verify `ui/uploads/bmadder-pi-prd.md` stays in sync (it currently lacks the typo — after fixing, both files' defaults sections must be byte-identical; if you can cheaply make `ui/uploads/` a build-time copy instead of a tracked twin, note it as a suggestion, do NOT restructure now).
2. Run `cargo fmt` once across the workspace (it currently fails `--check` on `bmadder-cli/src/agent.rs` import order + call wrapping). This is the ONLY project-wide formatter run permitted, and it must be its own commit.
3. `docs/incident-report-2026-07-23.md` Validation table says "3 pre-existing warnings"; after Task 1 the count changes. Append a section "## Post-Review Fixes (2026-07-23)" listing the tasks from this plan that were applied, and a one-line note that the incident report's Issue 5 description reflects a pre-consolidation moa-rust (the canonical moa-rust uses the aggregator's skill-as-preamble + a synthesis prompt that demands the deliverable; see the moa-rust repair plan). Do not rewrite history in the report body.

## Verification (run at the end, once)

```
cargo fmt --check               # clean (after Task 7.2)
cargo build                     # zero NEW warnings (AUTONOMOUS_MODE warning gone;
                                #   pre-existing spec.rs dead-code warnings may remain)
cargo test                      # all pass (51 baseline + new tests from Tasks 3, 6)
```

Then a smoke of the integration seam (requires the moa-rust plan to be done; skip with a note if it isn't):

```
cd /srv/data/1-projects/ai-projects/ai-r3
# dry-run only — must show the moa-rust engine labels and not error on arg parsing
<bmadder binary> iterative --dry-run
```

Report: per-task done/blocked status, new test names, and whether the integration smoke ran.