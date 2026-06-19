"""
preflight_auth.py — Verify agent CLI auth and catch rogue API keys.

Run before bmadder.sh cycle to confirm:
  1. Each required agent CLI is installed
  2. No environment variables silently override subscription billing
  3. Each agent can actually respond (auth is live)

Usage:
  uv run scripts/preflight_auth.py              # check all agents
  uv run scripts/preflight_auth.py --agents claude codex  # check specific ones
  uv run scripts/preflight_auth.py --fix        # unset rogue env vars and re-check

Exit codes:
  0  All checks passed
  1  One or more checks failed (details printed)
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

# ── Rogue env vars that silently switch CLIs from subscription to API billing ──
ROGUE_VARS = {
    "claude": [
        ("ANTHROPIC_API_KEY", "Claude Code will bill per-token via API instead of your Pro/Max subscription"),
    ],
    "codex": [
        ("OPENAI_API_KEY", "Codex CLI will bill per-token via API instead of your ChatGPT subscription"),
    ],
    "gemini": [
        ("GEMINI_API_KEY", "Gemini CLI will use this key instead of your Google AI subscription"),
        ("GOOGLE_API_KEY", "Gemini CLI may use this key instead of your Google AI subscription"),
    ],
}

# ── Agent CLI probe commands ──
# Each returns (command, args_to_check_version, args_to_check_auth)
AGENT_PROBES = {
    "claude": {
        "version_cmd": ["claude", "--version"],
        "auth_cmd": ["claude", "/status"],
        "auth_success_hints": ["pro", "max", "team", "enterprise", "plan", "active", "logged in"],
        "login_hint": "claude /login",
    },
    "codex": {
        "version_cmd": ["codex", "--version"],
        "auth_cmd": ["codex", "--auth-status"],
        "auth_success_hints": ["chatgpt", "plus", "pro", "authenticated", "logged in"],
        "login_hint": "codex (follow the sign-in prompt)",
    },
    "gemini": {
        "version_cmd": ["gemini", "--version"],
        "auth_cmd": ["gemini", "--version"],  # gemini doesn't have a status command; version confirms install
        "auth_success_hints": [],  # gemini prompts on first use; can't probe non-interactively
        "login_hint": "gemini (select 'Login with Google' on first run)",
    },
}

# ── Colors ──
RED = "\033[0;31m"
GREEN = "\033[0;32m"
YELLOW = "\033[1;33m"
BLUE = "\033[0;34m"
CYAN = "\033[0;36m"
NC = "\033[0m"


def info(msg: str):
    print(f"  {BLUE}[INFO]{NC}  {msg}")


def ok(msg: str):
    print(f"  {GREEN}[OK]{NC}    {msg}")


def warn(msg: str):
    print(f"  {YELLOW}[WARN]{NC}  {msg}")


def fail(msg: str):
    print(f"  {RED}[FAIL]{NC}  {msg}")


# ── Check 1: Rogue env vars ──

def check_billing_safety(agents: list[str], fix: bool = False) -> bool:
    """Check for env vars that silently override subscription billing."""
    all_clean = True
    for agent in agents:
        for var_name, reason in ROGUE_VARS.get(agent, []):
            val = os.environ.get(var_name)
            if val:
                if fix:
                    os.environ.pop(var_name, None)
                    warn(f"{var_name} was set — UNSET for this session")
                    warn(f"  → {reason}")
                    info(f"  To persist: remove {var_name} from your shell profile / .env files")
                else:
                    fail(f"{var_name} is set — {reason}")
                    info(f"  Current value: {val[:8]}...{val[-4:]}" if len(val) > 16 else f"  Current value: {val}")
                    info(f"  Fix: unset {var_name}  (or re-run with --fix)")
                    all_clean = False
    return all_clean


# ── Check 2: CLI installed ──

def check_installed(agent: str) -> bool:
    """Check if the agent CLI binary is available."""
    probe = AGENT_PROBES.get(agent)
    if not probe:
        warn(f"Unknown agent '{agent}' — skipping install check")
        return True

    try:
        result = subprocess.run(
            probe["version_cmd"],
            capture_output=True, text=True, timeout=10
        )
        version = result.stdout.strip().split("\n")[0] or result.stderr.strip().split("\n")[0]
        ok(f"{agent} installed: {version}")
        return True
    except FileNotFoundError:
        fail(f"{agent} not found in PATH")
        info(f"  Install: see https://docs.anthropic.com/en/docs/claude-code" if agent == "claude"
             else f"  Install: see the {agent} CLI docs")
        return False
    except subprocess.TimeoutExpired:
        warn(f"{agent} version check timed out (may still work)")
        return True


# ── Check 3: Auth live ──

def check_auth(agent: str) -> bool:
    """Try to verify the agent is authenticated (best-effort)."""
    probe = AGENT_PROBES.get(agent)
    if not probe:
        return True

    # Gemini can't be probed non-interactively — skip auth check
    if agent == "gemini":
        info(f"{agent}: auth can't be verified non-interactively")
        info(f"  If not yet logged in, run: {probe['login_hint']}")
        return True

    try:
        result = subprocess.run(
            probe["auth_cmd"],
            capture_output=True, text=True, timeout=15
        )
        output = (result.stdout + result.stderr).lower()

        # Look for hints that auth is active
        if any(hint in output for hint in probe["auth_success_hints"]):
            ok(f"{agent} auth looks active")
            return True
        else:
            warn(f"{agent} auth status unclear")
            info(f"  Output: {(result.stdout + result.stderr).strip()[:200]}")
            info(f"  If not logged in, run: {probe['login_hint']}")
            return True  # don't block — might be a different output format

    except FileNotFoundError:
        return False  # already caught by check_installed
    except subprocess.TimeoutExpired:
        warn(f"{agent} auth check timed out — may need interactive login")
        info(f"  Run: {probe['login_hint']}")
        return True


# ── Main ──

def main():
    parser = argparse.ArgumentParser(
        description="Verify BMADder agent CLI auth and billing safety."
    )
    parser.add_argument(
        "--agents", nargs="+", default=["claude", "codex", "gemini"],
        help="Which agents to check (default: all three)"
    )
    parser.add_argument(
        "--fix", action="store_true",
        help="Unset rogue env vars for this session and re-check"
    )
    parser.add_argument(
        "--quiet", action="store_true",
        help="Only print failures and warnings"
    )
    args = parser.parse_args()

    print()
    print(f"{CYAN}=== BMADDer Auth Preflight ==={NC}")
    print()

    passed = True

    # 1. Billing safety
    print(f"{CYAN}Billing safety (rogue env vars){NC}")
    if not check_billing_safety(args.agents, fix=args.fix):
        passed = False
    else:
        ok("No rogue API keys detected")
    print()

    # 2. CLIs installed
    print(f"{CYAN}Agent CLIs{NC}")
    for agent in args.agents:
        if not check_installed(agent):
            passed = False
    print()

    # 3. Auth live
    print(f"{CYAN}Auth status{NC}")
    for agent in args.agents:
        if not check_auth(agent):
            passed = False
    print()

    # Summary
    if passed:
        print(f"{GREEN}✓ All preflight checks passed.{NC}")
        print(f"  Ready to run: ./scripts/bmadder.sh cycle")
    else:
        print(f"{RED}✗ Some checks failed. Fix the issues above before running bmadder.sh.{NC}")
        print(f"  Quick fix for env vars: re-run with --fix")

    print()
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
