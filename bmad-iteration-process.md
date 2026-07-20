# BMad Post-MVP Iteration Process

How to keep developing with the BMad Method after the initial
`PRD → architecture → epics → sprint → implement` run is complete and the MVP is
deployed. This is project-agnostic — it documents the method, not this codebase.

**Status of "official" guidance:** BMad's installed catalog
(`_bmad/_config/bmad-help.csv`) defines four phases — `1-analysis`,
`2-planning`, `3-solutioning`, `4-implementation` — plus a set of `anytime`
skills. There is **no official "phase 5: iterate."** However, the method was
clearly built to cycle: `bmad-sprint-planning` is idempotent and never
downgrades statuses, `bmad-prd` has an *update* intent, `bmad-correct-course`
exists specifically for mid-flight change, `bmad-quick-dev` is the sanctioned
"anytime" dev entry point, and the **bmad-loop** module adds unattended
loop-until-done execution. The process below composes those official pieces
into a concrete iteration workflow, with enhancements marked as such.

---

## The core insight: three artifacts stay alive forever

Everything post-MVP hangs off three living artifacts. Keep them true and every
skill keeps working; let them rot and every skill starts hallucinating context.

| Artifact | Location | Kept alive by |
|---|---|---|
| Planning docs (PRD, architecture, epics) | `{planning_artifacts}` | `bmad-prd` (update intent), `bmad-architecture`, `bmad-correct-course` |
| Sprint state (`sprint-status.yaml`) | `{implementation_artifacts}` | `bmad-sprint-planning` (re-run anytime; preserves done/in-progress, carries `action_items`) |
| Deferred-work ledger (`deferred-work.md`) | `{implementation_artifacts}` | `bmad-quick-dev` / `bmad-dev-auto` append to it; `bmad-loop sweep` drains it |

**Rule zero (enhancement, but load-bearing):** after any significant merge or
manual change made outside BMad, refresh the ground truth —
`bmad-generate-project-context` (lean LLM rules file) and, when docs have
drifted badly, `bmad-document-project`. Also delete any stale
`epic-N-context.md` cache files in `{implementation_artifacts}` if planning
docs changed (the dev workflows invalidate them by timestamp, but a manual
`{planning_artifacts}` touch is what triggers recompilation).

---

## Step 1 of every iteration: triage the incoming work

Classify each incoming item (bug, feature request, idea, tech-debt) into one
of four tiers. This table is the whole routing decision:

| Tier | What it is | Route | Skills, in order |
|---|---|---|---|
| **T0 — Fix** | Bug or tweak with zero blast radius | Quick Dev, one-shot route | `bmad-quick-dev` (it routes to one-shot itself) |
| **T1 — Small feature** | Single user-facing goal, fits one spec (~900–1600 tokens), no PRD/architecture impact | Quick Dev, plan-code-review route | `bmad-quick-dev` → produces spec → implement → adversarial review → done |
| **T2 — Feature batch / new epic** | New capability worth stories; PRD scope grows but direction doesn't change | Mini planning pass, then the story cycle (or the loop — see below) | (`bmad-forge-idea`) → `bmad-prd` *update* → (`bmad-ux`) → (`bmad-architecture` if invariants change) → `bmad-create-epics-and-stories` (append Epic N+1) → `bmad-check-implementation-readiness` → `bmad-sprint-planning` → story cycle or `bmad-loop run` |
| **T3 — Direction change** | Something invalidates existing plan/scope (pivot, major rework, "the MVP taught us X") | Change management | `bmad-correct-course` → Sprint Change Proposal → routes itself: Minor→dev, Moderate→epics/backlog rework, Major→`bmad-prd`/`bmad-architecture` replan → then T2's tail |

Notes on the tiers:

- **T0/T1 share one entry point.** `bmad-quick-dev` clarifies intent, checks
  the multi-goal scope standard (and will offer to split, appending deferred
  goals to `deferred-work.md`), then routes itself between one-shot and
  plan-code-review. You don't pick the route; it does.
- **T2 is a compressed replay of phases 2→4**, scoped to the new epic. The key
  mechanic: `bmad-sprint-planning` re-run merges the new epic's stories into
  `sprint-status.yaml` as `backlog` **without touching** existing `done`
  statuses or retrospective `action_items`. That's the official mechanism for
  appending work.
- **`bmad-forge-idea` before T2 is optional but cheap insurance** — it
  pressure-tests the idea before you spend planning effort; a killed idea
  costs one conversation. Its `forged-idea.md` feeds `bmad-prd` or `bmad-spec`
  directly.
- **When unsure between T1 and T2:** if it's one shippable goal, T1. Multiple
  independently shippable deliverables, T2. `bmad-quick-dev`'s multi-goal
  check will catch a mis-triage and offer the split.

---

## The interactive story cycle (unchanged from phase 4)

For T2/T3 work driven by hand, the per-story cycle is exactly the official
implementation loop:

1. `bmad-sprint-status` — where are we, what's next, any open action items
2. `bmad-create-story` (create, then validate) — story file with full context
3. `bmad-testarch-atdd` — *(optional)* red-phase acceptance tests first
4. `bmad-dev-story` — implement tasks + tests
5. `bmad-code-review` — **fresh context window** (different LLM recommended);
   findings route back to `bmad-dev-story`, approval moves the story to `done`
6. Next story → back to 2. Epic complete → `bmad-retrospective` (action items
   land in `sprint-status.yaml`) → optionally `bmad-qa-generate-e2e-tests` /
   `bmad-testarch-automate` to backfill coverage on the shipped feature.

Run each skill in a fresh context window — the method assumes it, and the
artifacts (story files, specs, sprint-status) are the memory between windows.

---

## The loop: unattended feature-to-done (bmad-loop)

This is the "give it a feature and it loops until done" capability, and it is
an official module (BMAD Loop Skills), not an enhancement. The `bmad-loop`
orchestrator is a Python tool (installed via
`uv tool install "bmad-loop[tui] @ git+https://github.com/bmad-code-org/bmad-loop.git"`,
bootstrapped with `bmad-loop init`, or just run `/bmad-loop-setup` which does
all of it). It requires the bmm module and a `sprint-status.yaml`.

What it does per story, unattended:

```
sprint-status.yaml ──► spawn fresh CLI session ──► bmad-dev-auto  (dev pass:
        │                                          clarify → spec → implement)
        │                                              │ spec status: done
        │              spawn fresh CLI session ──► bmad-dev-auto  (review pass:
        │                                          adversarial review on the
        │                                          done spec, fixes)
        │                                              │
        ├── artifacts verified, hooks watched ─────────┘
        ├── CRITICAL escalation? ──► run PAUSES ──► human runs
        │                            /bmad-loop-resolve <story-key>
        │                            (fix the frozen spec together) ──► resume
        └── next story ──► repeat until sprint is done
```

Key properties worth knowing:

- **Escalation is the safety valve.** Automated sessions never guess on a
  contradictory or silent spec — they escalate, the run pauses, and
  `bmad-loop-resolve` is an *interactive* session where you disambiguate the
  spec's `<frozen-after-approval>` block. Then the orchestrator re-arms the
  story and rebuilds it against the corrected spec.
- **Deferred work is drained by the loop too.** `bmad-loop sweep` invokes
  `bmad-loop-sweep`, which verifies every open ledger entry against the actual
  code (statuses are known-unreliable), then partitions into: already-resolved
  (closed deterministically), buildable **bundles** (become dev sessions),
  blocked, skip, and human **decisions**. This is your automated tech-debt
  grinder.
- **Per-role CLI choice** lives in `.bmad-loop/policy.toml` — e.g. dev on
  `claude`, review on `codex` (fresh eyes across model families, the same
  principle as `bmad-code-review`'s "different LLM recommended").
- `bmad-loop validate` is the preflight; `bmad-loop tui` is the dashboard.

### The feature-to-done recipe (the concrete answer)

To hand BMad a feature and have it loop until done:

1. **Define** — T2 mini planning pass: `bmad-prd` update (or `bmad-spec` for a
   self-contained feature) → `bmad-create-epics-and-stories` to append the
   epic → `bmad-check-implementation-readiness`. This is the step that makes
   the loop safe: the loop's quality ceiling is the spec quality floor.
2. **Arm** — `bmad-sprint-planning` (new stories land as `backlog` /
   `ready-for-dev`), then `bmad-loop validate`.
3. **Run** — `bmad-loop run`. Walk away. Watch `bmad-loop tui` if you like.
4. **Unblock** — when it pauses, `/bmad-loop-resolve <story-key>`, decide,
   resume.
5. **Close** — `bmad-sprint-status`, `bmad-retrospective` on the epic,
   `bmad-loop sweep` to triage whatever the run deferred.

For a **single small feature** you don't need the loop at all —
`bmad-quick-dev` in one interactive session is faster. The loop earns its
setup cost at epic granularity or when grinding the deferred-work ledger.

---

## Recurring hygiene cadence (enhancement)

Not in the official catalog as a schedule; adopt as a habit. After each epic
(or every few weeks of T0/T1 churn):

1. `bmad-sprint-status` — surface risk and open action items
2. `bmad-loop sweep` (or manually read `deferred-work.md`) — drain the ledger
3. `bmad-retrospective` — if an epic closed since the last one
4. `bmad-generate-project-context` — refresh if the codebase moved a lot
5. `bmad-prd` *validate* — periodically confirm the PRD still describes the
   product you actually have; the MVP taught you things, encode them
6. Test posture: `bmad-testarch-trace` (coverage gate) and
   `bmad-testarch-nfr` (perf/security/reliability evidence) once real users
   exist — NFRs matter more post-deploy than pre-MVP

`bmad-help` remains the universal router: it reads the catalog plus your
artifacts on disk and recommends the next skill. When lost, start there.

---

## Anti-patterns

- **Skipping the PRD/epics update because "it's just one feature."** Then the
  next `bmad-correct-course` or `bmad-loop` run plans against a fiction.
  T1-sized work legitimately skips planning docs (the spec is the record);
  T2-sized work never should.
- **Editing `sprint-status.yaml` or a frozen spec's status by hand.** The
  orchestrator and skills own those state machines; edit content via the
  skills, statuses via the workflows.
- **Running the loop on vague stories.** Every escalation pause costs a human
  session. `bmad-check-implementation-readiness` before `bmad-loop run` is
  cheaper than three `bmad-loop-resolve` sessions.
- **Letting `deferred-work.md` grow silently.** The split mechanic in
  quick-dev makes deferring frictionless — which means the ledger fills up
  fast. Sweep on a cadence or it becomes a graveyard.
- **Reusing one long chat session across skills.** Fresh context per skill;
  the artifacts are the memory.

---

## Quick reference card

```
Bug / tweak                      → bmad-quick-dev
Small feature (one goal)         → bmad-quick-dev
New idea, unproven               → bmad-forge-idea → (spec or PRD update)
Feature batch / new epic         → bmad-prd(update) → [ux] → [architecture]
                                   → bmad-create-epics-and-stories
                                   → bmad-check-implementation-readiness
                                   → bmad-sprint-planning
                                   → bmad-loop run   (or manual story cycle)
Plan no longer matches reality   → bmad-correct-course
Loop paused (CRITICAL)           → /bmad-loop-resolve <story-key>
Tech-debt / deferred ledger      → bmad-loop sweep
Epic finished                    → bmad-retrospective
Where am I / what's next         → bmad-sprint-status, bmad-help
Codebase drifted from docs       → bmad-generate-project-context,
                                   bmad-document-project
```
