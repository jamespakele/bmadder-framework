# Changelog

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
