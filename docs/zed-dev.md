# BMADder-pi Implementation Checklist

> Based on `bmadder-pi-prd.md` v1.0  
> Check off each task as completed: `[x]`

---

## Phase 0 — Project Scaffolding

- [x] **0.1** Create Cargo workspace: `bmadder/Cargo.toml` with `[workspace]` members `bmadder-core` and `bmadder-cli`.
- [x] **0.2** Create `bmadder-core/Cargo.toml` (lib crate) with dependencies: `serde`, `serde_derive`, `serde_yaml`, `serde_json`, `toml`, `chrono`, `walkdir`.
- [x] **0.3** Create `bmadder-cli/Cargo.toml` (bin crate) with dependencies: `bmadder-core` (path), `clap` (v4, derive feature), `colored` (or `console`), `regex`, `sha2`, `tempfile`. Optional: `git2`.
- [x] **0.4** Create `bmadder-cli/src/main.rs` — minimal `fn main()` that prints "bmadder v0.1.0" and exits. Verify `cargo build` succeeds.
- [x] **0.5** Create `.gitignore` for the Rust workspace (`target/`, `Cargo.lock` if lib).

---

## Phase 1 — Core Types (`bmadder-core`)

- [x] **1.1** `src/story.rs` — Define `StoryStatus` enum: `Draft | Revise | ReadyForDev | InDev | PendingQA | Refix | Completed`. Derive `Debug, Clone, Serialize, Deserialize, PartialEq`. Implement `Display` for user-facing strings.
- [x] **1.2** `src/story.rs` — Define `StoryFrontmatter` struct with all fields: `story_id`, `epic_id` (Option), `title`, `status`, `priority` (Option), `agent_hint` (Option), `assigned_dev` (Option), `po_alignment` (Option), `qa_status` (Option), `created_at` (Option), `updated_at` (Option), `links` (Vec, default). All serde derive.
- [x] **1.3** `src/story.rs` — Define `Story` struct: `path: PathBuf`, `frontmatter: StoryFrontmatter`, `body: String`.
- [x] **1.4** `src/story.rs` — Unit tests for `StoryStatus`: verify all variants serialize/deserialize correctly. Verify `Display` output matches bash format.
- [x] **1.5** `src/config.rs` — Define `PathsConfig` struct: `skills_dir`, `headless_dir`, `stories_dir`, `state_dir`, `prd_file`, `architecture_file`, `orchestrator_marker` (all `PathBuf`).
- [x] **1.6** `src/config.rs` — Define `RoleConfig` struct: `personality: String`, `model: String`, `headless: String`.
- [x] **1.7** `src/config.rs` — Define `DefaultsConfig` struct: `max_dev_iterations`, `max_sm_iterations`, `max_qa_passes`, `story_timeout_seconds`, `gemini_cooldown_seconds`, `gemini_initial_backoff` (all u32 or u64).
- [x] **1.8** `src/config.rs` — Define `PiDevConfig` struct: `command: String`, `args: Vec<String>`.
- [x] **1.9** `src/config.rs` — Define `Config` struct: `project_root: PathBuf`, `paths: PathsConfig`, `models: HashMap<String, String>`, `roles: HashMap<String, RoleConfig>`, `agent_hints: HashMap<String, String>`, `defaults: DefaultsConfig`, `pi_dev: PiDevConfig`. Plus runtime overrides (CLI flags layered on top).
- [x] **1.10** `src/config.rs` — Implement `Config::load(path: &Path) -> Result<Config>`. Read TOML file, deserialize with `toml` crate, resolve all `[paths]` relative to config file's parent directory.
- [x] **1.11** `src/config.rs` — Implement `Config::apply_env_overrides()`. Read `BMADDER_*` env vars. Override `roles.*.model` with env var equivalents. Override `defaults.*` with `BMADDER_MAX_ITER`, etc.
- [x] **1.12** `src/config.rs` — Implement `Config::apply_cli_overrides()`. `--agent` overrides all role models. `--timeout` overrides `story_timeout_seconds`. `--max-iter` overrides `max_dev_iterations`. etc.
- [x] **1.13** `src/config.rs` — Implement `Config::resolve_model(phase: Phase, story: Option<&Story>) -> String`. Full priority chain: CLI flag > `BMADDER_AGENT` env > per-phase env > story `agent_hint` > TOML role default.
- [x] **1.14** `src/config.rs` — Implement `Config::resolve_personality_path(role_key: &str) -> PathBuf`. Joins `paths.skills_dir` + `roles[role_key].personality` + `SKILL.md`.
- [x] **1.15** `src/config.rs` — Implement `Config::resolve_headless_path(role_key: &str) -> PathBuf`. Joins `paths.headless_dir` + `roles[role_key].headless`.
- [x] **1.16** `src/config.rs` — Unit tests for config parsing: full TOML, minimal TOML with defaults, path resolution, env overlay, CLI overlay, model resolution with agent_hint.
- [x] **1.17** `src/agent.rs` — Define `AgentResult` struct: `success: bool`, `exit_code: i32`, `stdout: String`, `stderr: String`, `timed_out: bool`.
- [x] **1.18** `src/agent.rs` — Define `PiDevOutput` struct for JSON parsing (if pi.dev supports `--json-output`). Fields: `success`, `error`, `output_summary`.
- [x] **1.19** `src/lib.rs` — Re-export all public types from `story`, `config`, `agent`.

---

## Phase 2 — Story File I/O (`bmadder-cli`)

- [x] **2.1** `src/utils.rs` — Implement `find_project_root(start_dir: &Path) -> Option<PathBuf>`. Walk up directory tree looking for a file at `paths.orchestrator_marker` (default: `_bmad/orchestrator-master.md`). Returns the directory containing it.
- [x] **2.2** `src/utils.rs` — Implement `find_config(start_dir: &Path) -> Option<PathBuf>`. Walk up looking for `bmadder.toml`.
- [x] **2.3** `src/story_io.rs` — Implement `parse_story_file(path: &Path) -> Result<Story>`. Read file, detect `---` frontmatter fences, parse YAML block with `serde_yaml`, remainder is body.
- [x] **2.4** `src/story_io.rs` — Implement `write_story_file(story: &Story) -> Result<()>`. Serialize frontmatter to YAML, prepend `---\n` + YAML + `---\n`, append body, write to `story.path`.
- [x] **2.5** `src/story_io.rs` — Implement `update_story_status(path: &Path, new_status: StoryStatus) -> Result<()>`. Parse, update `frontmatter.status`, write back.
- [x] **2.6** `src/story_io.rs` — Implement `update_story_field(path: &Path, field: &str, value: &str) -> Result<()>`. Generic single-field update for frontmatter.
- [x] **2.7** `src/story_io.rs` — Implement `list_stories(stories_dir: &Path) -> Result<Vec<PathBuf>>`. Glob `story-*.md`, sort by filename (encodes dependency order).
- [x] **2.8** `src/story_io.rs` — Implement `get_stories_by_status(stories_dir: &Path, status: StoryStatus) -> Result<Vec<Story>>`. List all stories, parse each, filter by status.
- [x] **2.9** `src/story_io.rs` — Implement `count_by_status(stories_dir: &Path, status: StoryStatus) -> usize`. Count stories at given status.
- [x] **2.10** `src/story_io.rs` — Implement `filter_stories_by_id(stories: Vec<Story>, target_id: &str) -> Vec<Story>`. Keep only story with matching `story_id`.
- [x] **2.11** `src/story_io.rs` — Implement `filter_stories_from_id(stories: Vec<Story>, start_from: &str) -> Vec<Story>`. Skip stories before the given ID.
- [x] **2.12** `src/story_io.rs` — Unit tests: parse round-trip, update status, list and filter by status, sort order, invalid YAML handling.
- [x] **2.13** `src/story_io.rs` — Implement `validate_stories(stories_dir: &Path) -> Result<Vec<String>>`. Parse all stories, check required fields (`story_id`, `title`, `status`), check valid status values. Return list of errors.
- [x] **2.14** `src/story_io.rs` — Implement `detect_new_story_file(before: &[PathBuf], after: &[PathBuf]) -> Option<PathBuf>`. Diff two directory listings, return the new file path.

---

## Phase 3 — Agent Invocation (`bmadder-cli`)

- [x] **3.1** `src/agent.rs` — Implement `build_prompt_file(config: &Config, prompt_path: &Path, template: &str, variables: &HashMap<&str, &str>) -> Result<()>`. Substitute `{variable}` placeholders, write result to `_bmad/.prompt-tmp.md`.
- [x] **3.2** `src/agent.rs` — Implement `build_pi_dev_command(config: &Config, role_key: &str, prompt_file: &Path) -> Result<Command>`. Read `[roles.<key>]`, resolve personality path + headless path + model, substitute all `{...}` vars in `[pi_dev].args` template.
- [x] **3.3** `src/agent.rs` — Implement `invoke_agent(config: &Config, role_key: &str, prompt: &str) -> Result<AgentResult>`. Full pipeline: write prompt file → build command → spawn → wait with timeout → parse output → return result.
- [x] **3.4** `src/agent.rs` — Implement timeout enforcement: wrap subprocess wait in `std::time::Duration` timeout. Kill process on timeout. Return `AgentResult { timed_out: true, ... }`.
- [x] **3.5** `src/agent.rs` — Implement Gemini rate-limit detection: scan stdout/stderr for `429`, `rateLimitExceeded`, `MODEL_CAPACITY_EXHAUSTED`. Return special `AgentResult` variant or error.
- [x] **3.6** `src/agent.rs` — Implement exponential backoff state machine: `GeminiBackoff { current: Duration, max: Duration }`. `backoff()` doubles, `reset()` returns to initial. Thread-safe or per-story instance.
- [x] **3.7** `src/agent.rs` — Implement `run_with_retry(config, role_key, prompt, max_retries) -> Result<AgentResult>`. Wraps `invoke_agent`, detects rate-limit → backoff → retry. Respects `gemini_cooldown_seconds` between retries.
- [x] **3.8** `src/agent.rs` — Unit tests: prompt variable substitution, command-line construction, timeout behavior (mocked subprocess), rate-limit regex detection.

---

## Phase 4 — Logging (`bmadder-cli`)

- [x] **4.1** `src/logging.rs` — Implement `log_activity(config: &Config, actor: &str, story_id: &str, event: &str, detail: &str) -> Result<()>`. Append TSV line to `{state_dir}/logs/activity.log`. Format: `timestamp | actor | story_id | event | detail`.
- [x] **4.2** `src/logging.rs` — Implement `log_progress(config: &Config, message: &str) -> Result<()>`. Append timestamped line to `{state_dir}/progress.txt`.
- [x] **4.3** `src/logging.rs` — Implement console output helpers: `info(msg)`, `ok(msg)`, `warn(msg)`, `err(msg)`. Use `colored` crate. Match bash color scheme (BLUE, GREEN, YELLOW, RED).
- [x] **4.4** `src/logging.rs` — Implement `phase_banner(msg)` — cyan/magenta banner for phase headers.
- [x] **4.5** `src/logging.rs` — Implement `story_banner(msg)` — cyan box-drawing banner for iterative story start.
- [x] **4.6** `src/logging.rs` — Implement `show_status(config: &Config) -> Result<()>`. Print the full status table (Section 11.3 of PRD): story counts by status (color-coded), key file checks, agent routing display.
- [x] **4.7** `src/logging.rs` — Ensure all log directories are created on first write (`std::fs::create_dir_all`).

---

## Phase 5 — Git Integration (`bmadder-cli`)

- [x] **5.1** `src/git.rs` — Implement `git_add_all(project_root: &Path) -> Result<()>`. Run `git add -A`.
- [x] **5.2** `src/git.rs` — Implement `git_commit(project_root: &Path, message: &str) -> Result<()>`. Run `git commit -m "..."`. Allow empty commits.
- [x] **5.3** `src/git.rs` — Implement `git_push(project_root: &Path) -> Result<()>`. Best-effort `git push`. Log warning on failure, never halt pipeline.
- [x] **5.4** `src/git.rs` — Implement `git_snapshot(project_root: &Path) -> Result<()>`. `git add -A && git commit -m "chore: pre-dev worktree snapshot"` (allow empty). Used before dev loops.
- [x] **5.5** `src/git.rs` — Implement `git_story_commit(project_root: &Path, story_id: &str, title: &str) -> Result<()>`. `git add -A && git commit -m "story($story_id): $title [QA PASS]"`.
- [x] **5.6** `src/git.rs` — Implement `git_clean_worktree(project_root: &Path) -> Result<()>`. `git checkout .` to discard uncommitted code changes. Used on crash recovery when IN_DEV story is reset.
- [x] **5.7** `src/git.rs` — Implement `git_is_clean(project_root: &Path) -> bool`. Check if working tree is clean (no uncommitted changes).
- [x] **5.8** `src/git.rs` — Implement `git_init_if_needed(project_root: &Path) -> Result<bool>`. If no `.git` dir: `git init`, `git add -A`, `git commit -m "chore: initialize BMADder project"`. Return true if initialized.
- [x] **5.9** `src/git.rs` — Unit tests: all functions with a temp dir git repo fixture.

---

## Phase 6 — Phase: `plan` (`bmadder-cli`)

- [x] **6.1** `src/phases/plan.rs` — Implement `run_plan(config: &Config, skip_sm: bool, skip_po: bool) -> Result<()>`.
- [x] **6.2** Precondition checks: verify `prd.md` and `architecture.md` exist and have content.
- [x] **6.3** **Step 1 — SM (batch story creation):** If `!skip_sm`, build prompt using `roles.sm` + `sm-create-stories.md`. Prompt instructs SM to read PRD + arch, list existing stories, create new story files, handle REVISE stories, set `status: "DRAFT"`, `po_alignment: "PENDING"`, log to activity.log. Invoke `pi.dev`. Validate stories after SM completes.
- [x] **6.4** **Step 2 — PO (batch review):** If `!skip_po`, build prompt using `roles.po` + `po-review.md`. Prompt instructs PO to read ALL DRAFT stories, evaluate against 8 quality criteria, approve (`READY_FOR_DEV`, `APPROVED`) or reject (`REVISE`, `REVISE` with notes). Invoke `pi.dev`.
- [x] **6.5** Handle `--skip-po`: auto-approve all DRAFT stories → `READY_FOR_DEV`, `po_alignment: "APPROVED"`.
- [x] **6.6** Log final result: count READY_FOR_DEV vs REVISE. Write to `progress.txt` and `activity.log`.

---

## Phase 7 — Phase: `dev` (`bmadder-cli`)

- [x] **7.1** `src/phases/dev.rs` — Implement `run_dev(config: &Config, target_story: Option<&str>) -> Result<()>`.
- [x] **7.2** Pre-dev snapshot: commit uncommitted worktree changes (`git_snapshot`).
- [x] **7.3** Queue assembly: READY_FOR_DEV stories (sorted by filename) + REFIX stories appended. If `target_story`, filter to that story only.
- [x] **7.4** Per-story loop:
  - Resolve agent for THIS story: check `agent_hint` frontmatter → override model from `[agent_hints]` config.
  - Reset Gemini backoff state.
  - Update story status → `IN_DEV`.
  - Inner iteration loop (max `max_dev_iterations`):
    - Build prompt using `roles.dev` + `dev-story.md`. Prompt instructs Dev to read story, arch, prd, progress.txt, git log; implement with TDD; run build/test/lint; when done set `PENDING_QA` + fill Implementation Notes + commit.
    - Invoke `pi.dev`.
    - Read story status from disk. If `PENDING_QA` or `COMPLETED` → break.
    - If Gemini and more iterations remain → cooldown (from `gemini_cooldown_seconds`).
  - If max iterations reached without `PENDING_QA` → log STALLED.
- [x] **7.5** Log summary to `progress.txt`.

---

## Phase 8 — Phase: `qa` (`bmadder-cli`)

- [x] **8.1** `src/phases/qa.rs` — Implement `run_qa(config: &Config, target_story: Option<&str>) -> Result<()>`.
- [x] **8.2** Queue: all `PENDING_QA` stories. Filter if `target_story`.
- [x] **8.3** Per-story loop:
  - Resolve agent: `roles.qa`.
  - Build prompt using `roles.qa` + `qa-review.md`. Prompt instructs QA to read story ACs + Implementation Notes, review code, run tests, verify each AC, check for regressions. If all pass: `qa_status: "PASS"`, `status: "COMPLETED"`. If any fail: `qa_status: "FAIL"`, `status: "REFIX"` with detailed notes.
  - Invoke `pi.dev`.
  - **Enforce outcomes (bash-enforcer pattern):**
    - If status == `COMPLETED` → QA PASS: `git_story_commit`, `git_push`, log.
    - If status == `REFIX` → QA FAIL: log.
    - If status is ANYTHING ELSE → force `REFIX` + `qa_status: "FAIL"` (ambiguity protection).
- [x] **8.4** Handle `--no-commit` and `--dry-run` (skip git operations).

---

## Phase 9 — Phase: `cycle` (`bmadder-cli`)

- [x] **9.1** `src/phases/cycle.rs` — Implement `run_cycle(config: &Config) -> Result<()>`.
- [x] **9.2** If no READY_FOR_DEV or REFIX stories → run `plan` first. Smart SM skip: if DRAFTs exist and no REVISE, skip SM, run PO only.
- [x] **9.3** Loop up to `max_qa_passes` times: `run_dev()` → `run_qa()`.
- [x] **9.4** After each pass, count REFIX stories. If 0 → break.
- [x] **9.5** Final report: `show_status()`. If ALL stories COMPLETED: "ALL N STORIES COMPLETED". Otherwise: completed/total, stalled/REFIX/IN_DEV counts.

---

## Phase 10 — Phase: `iterative` (`bmadder-cli`)

- [x] **10.1** `src/phases/iterative.rs` — Implement `run_iterative(config: &Config, from_existing: bool, target_story: Option<&str>, start_from: Option<&str>) -> Result<()>`.
- [x] **10.2** Validate: `prd.md` + `architecture.md` exist. Run auth preflight. Pre-pipeline snapshot (`git_snapshot`).
- [x] **10.3** **Step 1 — Resume in-flight stories:**
  - If `--from-existing`: queue READY_FOR_DEV + REFIX.
  - Else: queue DRAFT + REVISE + READY_FOR_DEV + REFIX + IN_DEV + PENDING_QA.
  - Filter by `target_story` and/or `start_from`.
  - For each: call `process_one_story()`.
- [x] **10.4** **Step 2 — SM-driven loop (create new stories from PRD):**
  - While iterations < 100 (safety limit):
    - `sm_create_next_story()`: build prompt using `roles.sm_single` + `sm-create-story.md`. Prompt asks SM to read PRD + arch + progress.txt + git log, review existing stories. If unimplemented features exist, create ONE story file. If PRD fully implemented, write `ALL_DONE` to `progress.txt`.
    - Detect new story file (diff directory before/after).
    - If `ALL_DONE` in `progress.txt` → break.
    - If new story created → `process_one_story()`.
- [x] **10.5** **`process_one_story(story_file)`:**
  - Check current status.
  - **Phase 1 — SM↔PO Approval Loop** (for DRAFT or REVISE):
    - Loop up to `max_sm_iterations`:
      - `sm_write_story()`: SM creates/revises story content (same as Step 1 but targeted to one story). If DRAFT and empty → write full story. If REVISE → read PO notes, address every issue, set back to DRAFT.
      - If `--skip-po` → auto-approve, break.
      - `po_review_story()`: PO evaluates against 8 criteria. If all pass → `READY_FOR_DEV`, `APPROVED`. If any fail → `REVISE`, `REVISE` with notes.
    - If stalled → log STALLED, skip story.
  - **Phase 2 — Dev↔QA Implementation Loop** (for READY_FOR_DEV or REFIX or IN_DEV or PENDING_QA):
    - Loop up to `max_dev_iterations`:
      - Dev sub-loop (internal until PENDING_QA): same as `run_dev` per-story logic.
      - QA review: same as `run_qa` per-story logic.
      - If QA PASS → break.
      - If QA FAIL → status is REFIX, loop back to Dev.
    - If stalled → log STALLED.
  - If Dev↔QA passed: `commit_story()` → git commit + push. Log COMPLETE.
- [x] **10.6** Final report: completed this run, stalled, total COMPLETED on disk, PRD fully implemented check.

---

## Phase 11 — Phase: `status` (`bmadder-cli`)

- [x] **11.1** `src/phases/status.rs` — Implement `run_status(config: &Config) -> Result<()>`.
- [x] **11.2** Call `show_status()` from logging module (already implemented in 4.6).

---

## Phase 12 — Phase: `validate` (`bmadder-cli`)

- [x] **12.1** `src/phases/validate.rs` — Implement `run_validate(config: &Config) -> Result<()>`.
- [x] **12.2** Call `validate_stories()` from story_io module (already implemented in 2.13). Print errors or success message.

---

## Phase 13 — Bootstrap Module (`bmadder-cli`)

- [x] **13.1** `src/bootstrap.rs` — Implement `run_bootstrap(config_path: &Path, auto: bool) -> Result<()>`.
- [x] **13.2** **Step 1 — Folder structure:** Create `docs/backlog/stories/`, `docs/standards/`, `_bmad/logs/`. Use `create_dir_all`.
- [x] **13.3** **Step 2 — Orchestrator + standards:** Generate `_bmad/orchestrator-master.md` (marker file with minimal content). Generate standards files in `docs/standards/`.
- [x] **13.4** **Step 3 — Headless skills:**
  - Read `scripts/headless-skills/manifest.json`.
  - For each skill, check if output file exists and hash matches manifest.
  - If stale/missing: concatenate source files from `.agent/skills/` per manifest, prepend GENERATED header + HEADLESS MODE DIRECTIVES, write output.
  - Update manifest.json with new hashes.
- [x] **13.5** **Step 4 — Config files:**
  - Generate `bmadder.toml` with defaults (from Appendix A of PRD) if it does not exist. Never overwrite existing.
  - Update `.gitignore` with BMADder entries.
  - Generate `.mise.toml` if missing.
- [x] **13.6** **Step 5 — Tooling check:**
  - Check `pi.dev --version` (or `[pi_dev].command --version`) exists on PATH.
  - Check `git --version`.
  - Optional: `mise --version`, `uv --version`.
  - Report missing tools with install instructions.
- [x] **13.7** **Step 6 — Git init:** `git_init_if_needed()`.
- [x] **13.8** **Step 7 — Project files check:** Verify `docs/prd.md` and `docs/architecture.md` exist and have >500 bytes. Print ready message or TODO.
- [x] **13.9** Unit tests: bootstrap on empty temp dir → verify all files/dirs created.

---

## Phase 14 — Auth Preflight (`bmadder-cli`)

- [x] **14.1** `src/preflight.rs` — Implement `run_preflight(config: &Config) -> Result<()>`.
- [x] **14.2** Check `pi.dev` is on PATH and executable.
- [x] **14.3** Check `pi.dev` is authenticated (minimal no-op API call or version check).
- [x] **14.4** Check for rogue billing env vars: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`. Warn if any are set.
- [x] **14.5** Verify each model in `[models]` resolves to a known string (non-empty).
- [x] **14.6** Skip if `--dry-run` or `BMADDER_SKIP_PREFLIGHT=true`.

---

## Phase 15 — CLI Definition (`bmadder-cli/src/main.rs`)

- [x] **15.1** Define `clap` CLI with subcommands: `plan`, `dev`, `qa`, `cycle`, `iterative`, `status`, `validate`, `bootstrap`.
- [x] **15.2** Global flags (shared across subcommands): `--config <PATH>`, `--max-iter <N>`, `--max-sm-iter <N>`, `--max-dev-iter <N>`, `--dry-run`, `--skip-po`, `--skip-sm`, `--agent <AGENT>`, `--no-commit`, `--timeout <SECS>`, `--story <ID>`, `--from-existing`, `--start-from <ID>`, `--json`.
- [x] **15.3** Implement `main()`:
  1. Parse CLI args.
  2. Find `bmadder.toml` (via `--config` or auto-discover).
  3. Load `Config` from TOML.
  4. Apply env var overrides.
  5. Apply CLI flag overrides.
  6. Resolve project root.
  7. Dispatch to appropriate phase function.
  8. Handle errors: print to stderr, exit with appropriate code (0=success, 1=partial, 2=config error).
- [x] **15.4** Implement `--json` output mode: serialize results (status counts, errors) to JSON on stdout instead of colored text.
- [x] **15.5** Implement `--dry-run` mode: skip all `invoke_agent` calls. Print what WOULD run.

---

## Phase 16 — Integration & Wiring

- [x] **16.1** Wire auth preflight into `plan`, `dev`, `qa`, `cycle`, `iterative` phase entry points. Skip if `--dry-run`.
- [x] **16.2** Wire `--dry-run` through all phase functions: print actions, skip agent invocation and git operations.
- [x] **16.3** Wire `--json` through status display: serialize story counts and key file checks as JSON.
- [x] **16.4** Wire `--no-commit` through dev and qa phases: skip all git commit/push operations.
- [x] **16.5** Wire `--story` through dev, qa, and iterative: filter to single story.
- [x] **16.6** Wire `--start-from` through iterative: skip stories before given ID.
- [x] **16.7** Wire `--from-existing` through iterative: skip SM/PO loop.
- [x] **16.8** Wire `--skip-sm` through plan: use existing stories.
- [x] **16.9** Wire `--skip-po` through plan and iterative: auto-approve PO gate.
- [x] **16.10** Wire `--timeout` through agent invocation: override `story_timeout_seconds`.
- [x] **16.11** Wire `--max-iter`, `--max-sm-iter`, `--max-dev-iter` through respective loops.
- [x] **16.12** `--agent` (CLI flag) and `BMADDER_AGENT` (env): force all phase models. Determine the mapped pi.dev model via the `agent_hints` config section or direct model name lookup in `[models]`.

---

## Phase 17 — Prompt Templates

- [x] **17.1** `src/prompts.rs` — Implement `sm_batch_prompt(config, stories_exist: bool) -> String`. Build SM batch prompt from Section 7.1 of PRD. Include: role, headless skill reference, PRD + arch context, creation rules, revision handling, log instructions.
- [x] **17.2** `src/prompts.rs` — Implement `po_batch_prompt(config) -> String`. Build PO batch prompt. Include: role, headless skill reference, 8 quality criteria, approval/revision instructions.
- [x] **17.3** `src/prompts.rs` — Implement `dev_story_prompt(config, story: &Story) -> String`. Build Dev prompt. Include: story ID + file, arch + PRD + progress.txt context, `git log` instruction, TDD rules, completion criteria, constraints.
- [x] **17.4** `src/prompts.rs` — Implement `qa_story_prompt(config, story: &Story) -> String`. Build QA prompt. Include: story ID + file, AC review instructions, test suite run, regression check, PASS/FAIL criteria.
- [x] **17.5** `src/prompts.rs` — Implement `sm_single_prompt(config) -> String`. Build single SM prompt for iterative mode. Include: PRD + arch + progress.txt + git log, "create ONE story" instruction, "ALL_DONE" signal, dependency respect.
- [x] **17.6** `src/prompts.rs` — Implement `sm_write_story_prompt(config, story: &Story) -> String`. Build SM write/revise prompt for iterative SM↔PO loop. Handle DRAFT (write full) vs REVISE (address notes, set back to DRAFT).
- [x] **17.7** `src/prompts.rs` — Implement `po_single_prompt(config, story: &Story) -> String`. Build PO single-story review prompt. Same 8 criteria, single story only.

---

## Phase 18 — Testing

- [x] **18.1** Unit tests for `StoryStatus` serialization/deserialization/display.
- [x] **18.2** Unit tests for `StoryFrontmatter` parse/write round-trip with all fields populated.
- [x] **18.3** Unit tests for config: full TOML parse, minimal TOML with defaults, path resolution (relative → absolute).
- [x] **18.4** Unit tests for config: env var overlay priority.
- [x] **18.5** Unit tests for config: CLI flag overlay priority.
- [x] **18.6** Unit tests for config: agent_hint model resolution.
- [x] **18.7** Unit tests for story I/O: list, filter by status, filter by ID, filter start-from.
- [x] **18.8** Unit tests for story I/O: validate (missing fields, invalid statuses).
- [x] **18.9** Unit tests for story I/O: detect new file (before/after diff).
- [x] **18.10** Unit tests for agent: prompt variable substitution.
- [x] **18.11** Unit tests for agent: pi.dev command-line construction from template.
- [x] **18.12** Unit tests for agent: rate-limit regex detection (Gemini 429 patterns).
- [x] **18.13** Unit tests for git: init, snapshot, commit, clean, is_clean (with temp dir git repo).
- [x] **18.14** Unit tests for bootstrap: verify all files/dirs created on empty temp dir.
- [x] **18.15** Integration test: `bmadder plan --dry-run` on fixture project with PRD + arch → verify SM prompt content, PO prompt content.
- [x] **18.16** Integration test: `bmadder iterative --from-existing --dry-run` on fixture → verify story processing order, agent_hint overrides.
- [x] **18.17** Integration test: `bmadder status` → verify output format matches expected story counts.
- [x] **18.18** Integration test: `bmadder validate` → verify invalid status reported, valid status silent.
- [x] **18.19** Integration test: headless skill generation → verify output stripped of interactive artifacts, hash matches source.
- [ ] **18.20** End-to-end test: create "todo CLI app" project (PRD + arch + 5 stories), run `bmadder iterative`, verify all stories COMPLETED, git commits present, final MVP builds + tests pass.

---

## Phase 19 — Documentation

- [x] **19.1** `README.md` — Project overview, quick start (bootstrap → plan → iterative), CLI reference, config reference.
- [x] **19.2** `CHANGELOG.md` — Initial v0.1.0 entry.
- [x] **19.3** `bmadder-pi-prd.md` — Already written. Link from README.
- [x] **19.4** `zed-dev.md` — This checklist file. Self-referencing.

---

## Phase 20 — Polish & Release

- [x] **20.1** `cargo fmt` — Format all code.
- [x] **20.2** `cargo clippy` — Fix all warnings.
- [x] **20.3** `cargo test` — All tests pass.
- [x] **20.4** `cargo build --release` — Release binary builds.
- [ ] **20.5** Manual smoke test: bootstrap a fresh project, run `bmadder status`, run `bmadder validate`.
- [ ] **20.6** Tag v0.1.0 in git.

---

## Summary: Task Counts

| Phase | Tasks | Description |
|-------|-------|-------------|
| 0 | 5 | Project Scaffolding |
| 1 | 19 | Core Types (`bmadder-core`) |
| 2 | 14 | Story File I/O |
| 3 | 8 | Agent Invocation |
| 4 | 7 | Logging |
| 5 | 9 | Git Integration |
| 6 | 6 | Phase: `plan` |
| 7 | 5 | Phase: `dev` |
| 8 | 4 | Phase: `qa` |
| 9 | 5 | Phase: `cycle` |
| 10 | 6 | Phase: `iterative` |
| 11 | 2 | Phase: `status` |
| 12 | 2 | Phase: `validate` |
| 13 | 9 | Bootstrap Module |
| 14 | 6 | Auth Preflight |
| 15 | 5 | CLI Definition |
| 16 | 12 | Integration & Wiring |
| 17 | 7 | Prompt Templates |
| 18 | 20 | Testing |
| 19 | 4 | Documentation |
| 20 | 6 | Polish & Release |
| **Total** | **159** | |

---

*Generated: 2026-06-18*
*Source: bmadder-pi-prd.md v1.0*
