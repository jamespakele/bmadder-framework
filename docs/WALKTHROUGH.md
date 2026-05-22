# BMADDer End-to-End Walkthrough

From "I have an idea" to "I have a running MVP." Every command, every file, every decision point.

---

## Phase 0: Set Up Your Dev Environment

### Install mise (tool version manager)

```bash
curl https://mise.run | sh
echo 'eval "$(mise activate bash)"' >> ~/.bashrc
source ~/.bashrc
```

### Install uv (Python package manager)

```bash
mise use uv@latest
# or: curl -LsSf https://astral.sh/uv/install.sh | sh
```

### Install git

```bash
# Ubuntu/Debian
sudo apt install git

# macOS
brew install git
```

### Install agent CLIs

You need at least Claude Code. Codex is strongly recommended for dev. Gemini is optional.

**Claude Code** (plan + QA phases):
```bash
# See https://docs.anthropic.com/en/docs/claude-code
npm install -g @anthropic-ai/claude-code
claude /login
```

**Codex CLI** (dev phase):
```bash
# See https://github.com/openai/codex
npm install -g @openai/codex
# Auth happens on first use via browser
```

**Gemini CLI** (optional, UI only):
```bash
# See https://github.com/google-gemini/gemini-cli
npm install -g @anthropic-ai/gemini-cli  # Check the official repo for current install method
# Note: The actual package name may differ. See https://github.com/google-gemini/gemini-cli
# Auth happens on first use — select 'Login with Google'
```

### Verify everything

```bash
mise --version
uv --version
git --version
claude --version
codex --version   # optional
gemini --version  # optional
```

---

## Phase 1: Create a New Project

```bash
mkdir my-awesome-project
cd my-awesome-project
```

Copy the BMADDer scripts into your project:

```bash
cp -r /path/to/bmadder-framework/scripts ./scripts
```

Run bootstrap:

```bash
uv run scripts/bootstrap_bmadder.py
```

This creates:
- `docs/` — PRD, architecture, backlog, standards
- `_bmad/` — orchestrator contract, logs
- `src/` — your code will go here
- `scripts/` — already there from the copy
- `.mise.toml` — tool versions
- `.gitignore` — BMADDer-specific entries
- Initial git commit

Check the output. You should see `[OK]` for each step. Any `[MISS]` tools need to be installed before proceeding.

---

## Phase 2: Write Your PRD and Architecture

These are the two most important files in the entire process. Everything downstream depends on their quality.

### Option A: Generate with Perplexity (recommended)

Use Perplexity Computer (or any research/brainstorming tool) to iteratively build your PRD:

1. Start with your idea: "I want to build X that does Y for Z people"
2. Ask Perplexity to expand it into a full PRD with sections: Overview, Goals, Personas, Functional Requirements, Non-Functional Requirements, Constraints, Out of Scope
3. Iterate until the requirements are specific and testable
4. Do the same for the architecture: tech stack, data model, API design, infrastructure

Paste the results into `docs/prd.md` and `docs/architecture.md`.

### Option B: Write them manually

Open the template files created by bootstrap:

```bash
# These already have section headers — fill them in
$EDITOR docs/prd.md
$EDITOR docs/architecture.md
```

### Quality checklist

Before proceeding, verify:

- [ ] Every functional requirement is specific enough to write a test for
- [ ] The architecture specifies concrete technologies (not "we could use X or Y")
- [ ] The data model has actual entities and fields
- [ ] Out-of-scope is explicitly listed
- [ ] Non-functional requirements have numbers (e.g., "< 200ms response time" not "fast")

---

## Phase 3: Design Gate (Optional — UI Projects Only)

If your project has a UI, run the Google Stitch design gate before coding. This gives agents a visual reference to build from instead of inventing styles.

See [DESIGN-GATE.md](DESIGN-GATE.md) for the full Stitch workflow.

**Quick version:**

1. Open Google Stitch
2. Describe your UI in natural language, referencing your PRD
3. Export the design artifacts
4. Create `src/scaffolding/tokens.md` from the export (use `.deprecated/templates/tokens-template.md`)
5. Place layout/component templates in `src/scaffolding/layouts/` and `src/scaffolding/components/`

If you skip this step, frontend stories will still work — agents will just make their own design decisions (which may not be what you want).

---

## Phase 4: Auth Preflight

Before running any agents, verify auth and billing:

```bash
uv run scripts/preflight_auth.py
```

This checks three things:
1. **Billing safety** — no rogue env vars switching you from subscription to API billing
2. **CLIs installed** — agent binaries are in PATH
3. **Auth live** — agents can actually respond

If it finds rogue environment variables:

```bash
# Quick fix for this session
uv run scripts/preflight_auth.py --fix

# Permanent fix: remove the var from your shell profile
# e.g., remove "export ANTHROPIC_API_KEY=..." from ~/.bashrc
```

---

## Phase 5: Run the Cycle

### Full cycle (recommended first time)

```bash
./scripts/bmadder.sh cycle
```

This runs the complete pipeline:
1. **Plan**: SM creates stories from PRD + architecture, PO reviews and approves
2. **Dev**: Sequential dev loop, one story at a time, fresh context each iteration
3. **QA**: Sequential QA audit, one story at a time, Claude Opus deep review
4. **REFIX**: If QA fails any stories, loop back to Dev (max 3 passes)

### What to expect

- Plan phase takes 5-15 minutes depending on PRD complexity
- Each dev story takes 5-30 minutes depending on complexity and agent
- QA takes 3-10 minutes per story
- A 10-story project typically runs 1-3 hours end to end

### Watch it run

In another terminal:

```bash
# Check story states
./scripts/bmadder.sh status

# Watch the progress log
tail -f _bmad/progress.txt

# Watch the activity log
tail -f _bmad/logs/activity.log
```

---

## Phase 6: Monitor and Intervene

### Check status

```bash
./scripts/bmadder.sh status
```

This shows a dashboard: story counts per state, key file status, agent routing config.

### Read progress.txt

This is your primary debugging tool. Every dev and QA iteration appends what happened:

```
2026-03-14T22:15:01Z | STORY-0012: DEV iter 1 — database schema created (codex)
2026-03-14T22:16:00Z | STORY-0012: iter 2 — tests written, 4 passing, 2 failing (codex)
2026-03-14T22:22:00Z | STORY-0012: DEV done, 3 iters, codex
2026-03-14T22:25:31Z | STORY-0012: QA PASS
```

### Handle stalled stories

A story stalls when it hits `MAX_ITER` (default 10) without reaching PENDING_QA. The orchestrator logs `DEV_STALLED` and moves on.

To unstall:

1. Read `progress.txt` to understand what went wrong
2. Read the story's Implementation Notes to see what the agent tried
3. Consider:
   - Simplifying the story (split into smaller pieces)
   - Switching the agent (change `agent_hint` in frontmatter)
   - Adding implementation guidance to the story
4. Reset the story status:
   ```bash
   # Edit the story file manually
   # Set status: "READY_FOR_DEV"
   # Clear Implementation Notes if starting fresh
   ```
5. Re-run dev:
   ```bash
   ./scripts/bmadder.sh dev --story STORY-0031
   ```

### Handle REFIX loops

When QA fails a story, it goes to REFIX status with guidance in QA Notes. The cycle automatically loops REFIX → IN_DEV → PENDING_QA up to 3 times.

If a story keeps failing QA:

1. Read QA Notes for the specific failure
2. Read the code and tests
3. Consider adding more explicit implementation guidance to the story
4. Consider changing the agent hint (e.g., from codex to claude for complex logic)
5. Re-run:
   ```bash
   ./scripts/bmadder.sh dev --story STORY-0043
   ./scripts/bmadder.sh qa --story STORY-0043
   ```

---

## Phase 7: Run Partial Cycles

You don't have to run the full cycle every time. Common partial runs:

### Dev only (stories already planned and approved)

```bash
./scripts/bmadder.sh dev
```

### QA only (stories already developed)

```bash
./scripts/bmadder.sh qa
```

### Single story

```bash
./scripts/bmadder.sh dev --story STORY-0015
./scripts/bmadder.sh qa --story STORY-0015
```

### Plan only (just create stories, no dev)

```bash
./scripts/bmadder.sh plan
```

### Plan without PO gate (rapid prototyping)

```bash
./scripts/bmadder.sh plan --skip-po
```

### Dry run (see what would execute)

```bash
./scripts/bmadder.sh cycle --dry-run
```

### Force a specific agent

```bash
# Use Claude for everything (good for debugging)
./scripts/bmadder.sh cycle --agent claude

# Increase max iterations for stubborn stories
./scripts/bmadder.sh dev --max-iter 15 --story STORY-0031
```

---

## Phase 8: Post-MVP Review

After all stories are COMPLETED:

### 1. Read activity.log for the full decision audit

```bash
cat _bmad/logs/activity.log
```

This shows every phase start/end, every QA pass/fail, every git push. It's the audit trail.

### 2. Read progress.txt for the development narrative

```bash
cat _bmad/progress.txt
```

This shows iteration-by-iteration progress: what each agent did, how many iterations, where things got stuck.

### 3. Check for stalled stories

```bash
./scripts/bmadder.sh status
```

Any stories not in COMPLETED need manual attention. Check their frontmatter, Implementation Notes, and QA Notes.

### 4. Review the code

The agents committed code after each QA pass. Review the git history:

```bash
git log --oneline
```

Each commit follows the pattern: `story(STORY-NNNN): title [QA PASS]`

### 5. Run the test suite yourself

```bash
# Whatever your project's test command is
cargo test    # Rust
npm test      # Node
pytest        # Python
```

### 6. Validate all story frontmatter

```bash
./scripts/bmadder.sh validate
```

Confirms all COMPLETED stories have `qa_status: "PASS"` and other consistency checks.

---

## Troubleshooting

### "Not a BMADder project"

Run bootstrap first: `uv run scripts/bootstrap_bmadder.py`

### Agent hangs / doesn't exit

- Claude/Gemini: Should be fine with `< /dev/null` redirect (handled by orchestrator)
- Codex: Must use `codex exec --full-auto` (handled by orchestrator). If it still hangs, kill the process and check your codex version.

### Terminal is garbled after Codex run

Codex can leave the terminal in raw mode. The orchestrator runs `stty sane` after each Codex invocation, but if something goes wrong:

```bash
stty sane
reset
```

### Agent killed by timeout

Default is 900s (15 min). Increase it:

```bash
./scripts/bmadder.sh dev --timeout 1800   # 30 minutes
```

### Stories created but none approved

PO gate found issues. Read the story files — look at `## PO Alignment` for revision notes. Fix the issues and re-run plan, or skip PO:

```bash
./scripts/bmadder.sh plan --skip-po
```

### Rogue API key warning

See the Auth & Billing section in the README. Quick fix:

```bash
uv run scripts/preflight_auth.py --fix
```
