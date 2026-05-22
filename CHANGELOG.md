# Changelog

## v3.0 (2026-05-22)
- Added `sync_headless_skills.py` to synchronize and convert interactive BMad skills from `.agent/skills/` into non-interactive Markdown prompts in `scripts/headless-skills/`
- Integrated automatic headless skill sync checking as Step 3 in `bootstrap_bmadder.py`
- Added `bmadder-iterative.sh` supporting iterative, story-at-a-time execution of the dev/qa cycle (plan all → per-story dev/qa lifecycle)
- Renamed framework runtime directory from `.bmad/` to `_bmad/` across all scripts and documents
- Updated orchestrator prompts in `bmadder.sh` and `bmadder-iterative.sh` to reference the synced headless skills
- Deprecated legacy scripts (`lint.mjs`, `turbo-shim.mjs`) and the `templates/` folder, moving them to `.deprecated/`
- Performed exhaustive documentation update across all guides (READMEs, Design Gate, Walkthrough)

## v2.0 (2026-03-14)
- Extracted framework from hpd-alerts project into standalone package
- Added `codex exec --full-auto` mode (fixes interactive hang)
- Added `stty sane` TTY restoration after Codex runs
- Added `timeout` per agent invocation (default 900s)
- Added pre-dev worktree commit to prevent agent stdin prompts
- Added `preflight_auth.py` for billing safety checks
- Fixed bash arithmetic with `set -e` (iter++ || true)
- Fixed model flag format (--model sonnet, not --model="sonnet")
- Removed non-existent --no-input and -q flags
- Added `|| true` on run_agent to prevent orchestrator crash on agent failure
- Documentation: full walkthrough, agent compatibility guide, lessons learned

## v1.0 (2026-03-06)
- Initial framework design
- Orchestrator-master.md state machine
- Bootstrap, init, create_rules, validate scripts
- bmadder.sh with plan/dev/qa/cycle phases
