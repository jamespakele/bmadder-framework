# Lessons Learned

Hard-won knowledge from the first real BMADDer test run — building an HPD Alerts application from scratch using the v1.0 framework. Everything that worked, everything that broke, and what we changed.

---

## The Test Run

The HPD Alerts project was a full-stack application built entirely by BMADDer agents. PRD and architecture were generated with Perplexity, stories were created by Claude Sonnet (SM), reviewed by Claude Sonnet (PO), developed by Codex, and audited by Claude Opus (QA).

Total stories: ~45. Completed autonomously: ~35. Required intervention: ~10.

---

## What Went Right

### Codex + TDD = excellent compliance

Stories 0010 through 0022 — database schema, core models, basic API endpoints — all completed in 1 iteration each via Codex. The pattern was consistent: Codex read the story, wrote failing tests, implemented until they passed, ran build/test/lint, marked PENDING_QA. Textbook.

This validated the core thesis: if you give an agent clear acceptance criteria and a TDD mandate, it can reliably implement straightforward features without human intervention.

### Claude Sonnet for planning was cost-effective and high-quality

The SM phase consistently produced well-structured, atomic stories with proper dependency ordering. The PO gate caught real issues — cross-story inconsistencies, missing dependency stories, overly broad acceptance criteria.

The Sonnet model hit the sweet spot: smart enough for structured doc generation, cheap enough to run 50+ invocations without worrying about cost.

### The PO gate caught real problems

On the first run, the PO identified:
- Two stories that overlapped in scope (both claimed to implement user preferences)
- A missing story for database migrations that three other stories depended on
- Acceptance criteria that were untestable ("the UI should feel responsive")

These would have caused cascading failures in dev. The PO gate is not optional.

### progress.txt was the primary debugging tool

When stories stalled, `progress.txt` was how we figured out what happened. Each iteration's notes showed:
- What the agent tried
- What worked and what didn't
- Where it got stuck
- What it planned to do next

Without `progress.txt`, debugging a stalled story would mean reading through 10 iterations of git diffs. With it, you can understand the trajectory in 30 seconds.

### The pre-dev worktree commit saved the run multiple times

Before we added the automatic `git add -A && git commit -m "chore: pre-dev worktree snapshot"` before each dev loop, Codex would sometimes prompt about dirty files and hang. This one-line fix eliminated an entire class of failures.

### Git commit after QA pass was the right call

Having the orchestrator (not the QA agent) handle git commit + push after QA pass was correct. Early versions let agents run git, which caused:
- Agents committing partial work
- Agents committing with wrong message format
- Agents pushing to wrong branch

The orchestrator commits with a consistent message format: `story(STORY-NNNN): title [QA PASS]`

---

## What Went Wrong

### Codex couldn't handle complex cross-module dependencies

Stories 0031, 0032, 0040, and 0042 all STALLED at 10 iterations. The pattern: these stories required understanding and modifying code across multiple modules simultaneously. Codex would fix one module, break another, fix that one, break the first again. Classic thrashing.

The fix: switch `agent_hint` to `claude` for stories with complex cross-module dependencies. Story 0043 (similar complexity to 0031) completed in 1 iteration via Claude.

**Takeaway:** Codex excels at focused, single-module work. Claude excels at cross-cutting concerns. Route accordingly.

### The Codex interactive hang consumed hours

The initial approach used `codex "prompt"` which launched the TUI. In a headless environment, this hung forever (until timeout). We tried:
- `codex -q "prompt"` — flag doesn't exist
- `codex --no-input "prompt"` — flag doesn't exist
- `codex "prompt" < /dev/null` — didn't work (Codex doesn't support stdin redirect)

The fix was `codex exec --full-auto "prompt"` — a completely different invocation mode that skips the TUI entirely. This wasn't documented prominently and cost significant debugging time.

**Takeaway:** Test agent CLI invocation modes thoroughly before a long run. The happy-path interactive mode is often different from the headless mode.

### The `set -e` + bash arithmetic bug

```bash
((iter++))  # When iter is 0, this returns exit code 1 → kills the script
```

With `set -e`, any non-zero exit kills the script. Bash arithmetic returns 1 when the result is 0 (which happens when incrementing from 0, because the pre-increment value IS 0). Fix:

```bash
(( iter++ )) || true
```

This is a well-known bash gotcha but easy to miss. It killed the orchestrator mid-run before we caught it.

### Agent failure crashed the orchestrator

Similarly, if an agent exited non-zero (which happens routinely — timeout, rate limit, partial completion), `set -e` killed the entire orchestrator. Fix:

```bash
run_agent "$pf" "dev" || true
```

The orchestrator checks the story's status on disk after each invocation anyway. A non-zero exit from the agent doesn't mean the work wasn't done — it often means the agent timed out after completing work but before cleanly exiting.

### TTY corruption after Codex was mystifying

After Codex runs, the terminal would be garbled: keystrokes didn't echo, line editing was broken, escape sequences didn't work. The orchestrator continued running (bash doesn't need a clean TTY) but human interaction was impossible.

Root cause: Codex puts the terminal in raw mode for its TUI and doesn't always restore it on exit, especially under timeout kills.

Fix: Save and restore TTY state, plus `stty sane` as a safety net.

### Rate limiting was unpredictable

Gemini rate-limited frequently under sustained use. It self-recovered (internal retry), but the variable latency made timing unpredictable. Codex rate-limited rarely. Claude almost never.

**Takeaway:** Build timeouts generous enough to absorb rate-limit delays. The default 900s (15 min) worked well.

### Rogue ANTHROPIC_API_KEY caused unexpected billing

A development machine had `ANTHROPIC_API_KEY` set in `.bashrc` from a previous project. Claude Code silently picked it up and billed per-token instead of using the Pro subscription. A full cycle generated a non-trivial API bill before anyone noticed.

This led directly to `preflight_auth.py` — the billing safety check that runs before any cycle.

---

## Design Decisions That Held Up

### Fresh context per invocation

The "no conversation history" rule was controversial but correct. Every agent invocation starts clean and discovers state from the filesystem. This means:
- No hallucination drift across iterations
- Every invocation is independently reproducible
- If an agent goes off the rails in one iteration, the next one starts fresh
- Debugging is straightforward — you can reproduce any iteration by checking out that git commit

### Bash as state machine, not LLM

The orchestrator script reads frontmatter on disk to decide what happens next. It never trusts the agent's claim about what it did. If an agent says "I moved the story to PENDING_QA" but the file still says IN_DEV, bash catches it and keeps looping.

This separation of concerns — LLM does work, bash enforces workflow — is the most important architectural decision in the framework.

### Sequential processing

Processing stories one at a time in dependency order prevents merge conflicts and cross-story contamination. It's slower than parallel execution, but it works correctly every time. When your agents are running headless for hours, "works correctly" beats "works fast" every time.

### Filesystem as memory

No database, no service, no hidden state. Everything is in files under version control:
- `progress.txt` — dev narrative
- `activity.log` — structured audit trail
- Story frontmatter — state machine
- `git log` — source of truth

This means you can understand, debug, and reproduce any run by looking at files. You can also back up, restore, and share project state trivially.

---

## Configuration Changes Made During the Run

| Change | Before | After | Why |
|--------|--------|-------|-----|
| Codex invocation | `codex "prompt"` | `codex exec --full-auto "prompt"` | Interactive hang |
| TTY handling | Nothing | `stty -g` save + `stty sane` restore | Terminal corruption |
| Bash arithmetic | `((iter++))` | `(( iter++ )) \|\| true` | set -e crash |
| Agent failure | `run_agent "$pf"` | `run_agent "$pf" \|\| true` | set -e crash |
| Model flag | `--model="sonnet"` | `--model sonnet` | Silent flag parsing failure |
| Pre-dev commit | None | `git add -A && git commit` | Codex dirty-worktree prompt |
| Timeout | None | `timeout 900` per invocation | Infinite hang prevention |
| Auth check | None | `preflight_auth.py` | Rogue API key billing |

---

## Recommendations for New Projects

1. **Start with a solid PRD.** Vague requirements produce vague stories that confuse agents. Spend the time upfront.

2. **Don't skip the PO gate.** It catches real problems. Use `--skip-po` only for rapid prototyping.

3. **Default to Codex for dev, but switch to Claude for complex stories.** The `agent_hint` field exists for a reason.

4. **Read progress.txt when stories stall.** It's always more informative than git diff.

5. **Run preflight_auth.py before every cycle.** The cost of a rogue API key bill is not trivial.

6. **Increase timeout for complex stories.** 15 minutes is fine for simple stories. Complex ones may need 30 minutes.

7. **Don't fight the state machine.** If a story keeps failing, split it. The framework works best with small, focused stories.

8. **Review the code after.** Agents write code, but you should still read it. QA catches most issues, but it's your project.
