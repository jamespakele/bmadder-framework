# Agent Compatibility Guide

Practical notes on each agent CLI's behavior, quirks, and workarounds. Everything here was discovered the hard way.

---

## Quick Reference

| Feature | Claude Code | Codex CLI | Gemini CLI |
|---------|-------------|-----------|------------|
| **stdin redirect** | `< /dev/null` works | NO stdin redirect | `< /dev/null` works |
| **Model flags** | `--model sonnet`, `--model opus` | None (positional args only) | None needed |
| **TTY required** | No | Yes (raw mode) | No |
| **Non-interactive mode** | `--dangerously-skip-permissions -p "prompt"` | `codex exec --full-auto "prompt"` | `--yolo -p "prompt"` |
| **Auto-exits** | Yes | Yes (with `exec` mode) | Yes |
| **Timeout behavior** | Clean exit on SIGTERM | May leave TTY dirty | Clean exit on SIGTERM |
| **Auth probe** | `claude /status` | `codex --auth-status` | Can't probe non-interactively |
| **Billing risk** | `ANTHROPIC_API_KEY` env var | `OPENAI_API_KEY` env var | `GEMINI_API_KEY` / `GOOGLE_API_KEY` |
| **Subscription** | Claude Pro/Max | ChatGPT Plus/Pro | Google AI subscription |

---

## Claude Code

**Used for:** Plan (SM + PO) with Sonnet, QA with Opus.

### Invocation

```bash
claude --dangerously-skip-permissions --model sonnet -p "prompt" < /dev/null
claude --dangerously-skip-permissions --model opus -p "prompt" < /dev/null
```

### Key behaviors

- `--dangerously-skip-permissions` is required for non-interactive use. Without it, Claude will prompt for file access permissions.
- `< /dev/null` prevents Claude from blocking on stdin after completion. Without it, the process may hang waiting for input.
- Model flag format is `--model sonnet` (space-separated). NOT `--model="sonnet"` or `--model=sonnet`.
- `--model sonnet` resolves to the latest Sonnet. `--model opus` resolves to the latest Opus. You don't need to specify exact model IDs.
- Claude Code respects `ANTHROPIC_API_KEY` if set, which switches from subscription billing to per-token API billing. The preflight check catches this.

### Known issues

- None significant. Claude Code is the most reliable agent in the stack.
- Sonnet is cost-effective for planning. Opus provides the depth needed for QA.

---

## Codex CLI

**Used for:** Dev phase (default agent for most stories).

### Invocation

```bash
codex exec --full-auto "prompt"
```

### Key behaviors

- `codex exec --full-auto` is the ONLY reliable non-interactive mode. This was the single most important discovery during testing.
- **Do NOT use stdin redirect** (`< /dev/null`) with Codex. It doesn't work and causes silent failures.
- **Do NOT use** `codex -q`, `codex --quiet`, or `codex --no-input`. These flags don't exist. They were hallucinated by planning agents and caused failures.
- Codex puts the terminal in raw mode during execution. After it exits, the terminal may be garbled — characters don't echo, line editing breaks.
- The orchestrator saves TTY state before Codex and restores it after. It also runs `stty sane` as a safety net.
- Codex is excellent at TDD compliance. It reliably writes failing tests first, then implements until they pass.

### The interactive hang bug

Before `exec --full-auto` was discovered, Codex would launch its TUI (terminal UI) and wait for user interaction. Since BMADDer runs headless, this caused the process to hang until timeout killed it.

**Fix:** Use `codex exec --full-auto` exclusively. This runs Codex in execution mode where it processes the prompt, does the work, and exits. No TUI, no interaction.

### Pre-dev worktree commit

Codex can prompt about dirty/uncommitted files in the working tree. Since there's no human to respond, this causes a hang.

**Fix:** The orchestrator commits all uncommitted changes before entering the dev loop:

```bash
git add -A && git commit -m "chore: pre-dev worktree snapshot"
```

This ensures Codex starts with a clean worktree every time.

### TTY restoration

```bash
# Save before
tty_settings=$(stty -g 2>/dev/null)

# Run codex
codex exec --full-auto "prompt"

# Restore after
stty "$tty_settings" 2>/dev/null
stty sane 2>/dev/null
```

### Known issues

- Leaves terminal in raw mode (handled by `stty sane`)
- Can't redirect stdin (use `exec --full-auto` instead)
- May struggle with complex cross-module dependencies (consider switching to Claude for those stories)
- Rate limiting is rare but possible on heavy usage

---

## Gemini CLI

**Used for:** Dev phase, UI/UX stories only (rare).

### Invocation

```bash
gemini --yolo -p "prompt" < /dev/null
```

### Key behaviors

- `--yolo` mode auto-approves all file operations. Required for non-interactive use.
- `< /dev/null` prevents stdin blocking (same as Claude).
- Gemini's auth can't be probed non-interactively. The first run triggers a browser-based Google login flow. Run `gemini` once manually before using BMADDer.
- Rate limits more frequently than Claude or Codex. It self-recovers (retries automatically), but this adds latency.

### When to use Gemini

Almost never. The default is Codex for all stories, including frontend. Gemini is only relevant when:
1. No Stitch scaffolding exists
2. You need multimodal UI generation from scratch
3. You explicitly set `agent_hint: "gemini"` on a story

In practice, Codex handles frontend fine when given Stitch design tokens and scaffolding as reference.

### Known issues

- Rate limiting under heavy load (self-recovers with delay)
- Can't verify auth non-interactively — must do first login manually
- Less reliable for backend/logic tasks than Claude or Codex
- Two possible rogue env vars: `GEMINI_API_KEY` and `GOOGLE_API_KEY`

---

## Bug Reference

Issues discovered and fixed during testing:

| Bug | Agent | Symptom | Fix |
|-----|-------|---------|-----|
| Interactive hang | Codex | Process hangs at TUI, never processes prompt | Use `codex exec --full-auto` |
| TTY corruption | Codex | Terminal garbled after run (no echo, no line editing) | `stty sane` after every Codex invocation |
| Dirty worktree prompt | Codex | Codex prompts about uncommitted files, hangs | Pre-dev `git add -A && git commit` |
| Non-existent flags | Codex | `--no-input`, `-q` flags cause immediate error | Remove — these flags don't exist |
| Model flag format | Claude | `--model="sonnet"` fails silently | Use `--model sonnet` (space-separated) |
| Stdin block | Claude/Gemini | Process hangs after work completes | Redirect `< /dev/null` |
| Bash arithmetic + set -e | All | `((iter++))` returns 1 when iter was 0, kills script | Use `(( iter++ )) || true` |
| Agent failure kills orchestrator | All | Non-zero exit from agent crashes `set -e` script | Use `run_agent ... || true` |
| Rogue API keys | All | Silent billing switch from subscription to per-token | `preflight_auth.py` check |

---

## Agent Selection Guidelines

| Story Type | Recommended Agent | Why |
|-----------|-------------------|-----|
| Database schema, models | codex | Strong at structured code generation |
| REST API endpoints | codex | Good at boilerplate + test generation |
| Frontend pages (with Stitch) | codex | References scaffolding templates reliably |
| Complex business logic | claude | Better at reasoning across modules |
| Data transformations | claude | Better at algorithm design |
| Config/infrastructure | claude | Better at understanding system context |
| UI from scratch (no Stitch) | gemini | Multimodal, can reason about visual design |
| All QA | claude (opus) | Deep reasoning, catches subtle issues |
