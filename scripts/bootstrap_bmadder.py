"""
bootstrap_bmadder.py — One-command BMADder project setup.
Creates folder structure, generates rules/standards, verifies tooling.

Usage:
  uv run scripts/bootstrap_bmadder.py          # interactive
  uv run scripts/bootstrap_bmadder.py --auto    # non-interactive (CI/scripts)

What it does:
  1. Creates folder structure (init_bmadder.py)
  2. Creates orchestrator + standards files (create_rules.py)
  3. Verifies mise, uv, git are available
  4. Initializes git repo if not already one
  5. Makes bmadder.sh executable
"""

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run_script(name: str):
    """Run a sibling script via uv."""
    script = ROOT / "scripts" / name
    if not script.exists():
        print(f"[ERROR] {script} not found.")
        sys.exit(1)
    try:
        subprocess.run(
            ["uv", "run", str(script)],
            check=True,
            text=True,
            cwd=str(ROOT),
        )
    except FileNotFoundError:
        # uv not available, fall back to direct python
        subprocess.run(
            [sys.executable, str(script)],
            check=True,
            text=True,
            cwd=str(ROOT),
        )
    except subprocess.CalledProcessError:
        print(f"[ERROR] {name} failed.")
        sys.exit(1)


def check_tool(name: str, check_cmd: list[str]) -> bool:
    """Check if a CLI tool is available."""
    try:
        result = subprocess.run(
            check_cmd,
            capture_output=True, text=True, timeout=10
        )
        version = result.stdout.strip().split("\n")[0]
        print(f"  [OK]   {name}: {version}")
        return True
    except (FileNotFoundError, PermissionError, subprocess.TimeoutExpired):
        print(f"  [MISS] {name}: not found")
        return False


def init_git():
    """Initialize git repo if not already one."""
    git_dir = ROOT / ".git"
    if git_dir.exists():
        print("  [OK]   Git repo exists.")
        return

    print("  [INIT] Initializing git repo...")
    subprocess.run(["git", "init"], cwd=str(ROOT), check=True, capture_output=True)
    subprocess.run(["git", "add", "-A"], cwd=str(ROOT), check=True, capture_output=True)
    subprocess.run(
        ["git", "commit", "-m", "chore: initialize BMADder project"],
        cwd=str(ROOT), check=True, capture_output=True
    )
    print("  [OK]   Git repo initialized with initial commit.")


def make_executable():
    """Make bmadder.sh executable."""
    sh = ROOT / "scripts" / "bmadder.sh"
    if sh.exists():
        sh.chmod(sh.stat().st_mode | 0o755)
        print("  [OK]   scripts/bmadder.sh is executable.")


def write_gitignore():
    """Add BMADder-specific entries to .gitignore."""
    gitignore = ROOT / ".gitignore"
    entries = [
        "# BMADder",
        "_bmad/.prompt-tmp.md",
        "__pycache__/",
        "*.pyc",
        ".venv/",
    ]
    existing = gitignore.read_text(encoding="utf-8") if gitignore.exists() else ""
    to_add = [e for e in entries if e not in existing]
    if to_add:
        with open(gitignore, "a", encoding="utf-8") as f:
            if existing and not existing.endswith("\n"):
                f.write("\n")
            f.write("\n".join(to_add) + "\n")
        print("  [OK]   .gitignore updated.")
    else:
        print("  [SKIP] .gitignore already has BMADder entries.")


def write_mise_toml():
    """Create .mise.toml if it doesn't exist."""
    mise_toml = ROOT / ".mise.toml"
    if mise_toml.exists():
        print("  [SKIP] .mise.toml already exists.")
        return
    mise_toml.write_text("""[tools]
python = "3.12"
uv     = "latest"
rust   = "stable"
node   = "latest"

[settings]
python.uv_venv_auto = true
""", encoding="utf-8")
    print("  [OK]   Created .mise.toml")


def main():
    parser = argparse.ArgumentParser(description="Bootstrap a BMADder project.")
    parser.add_argument("--auto", action="store_true",
                        help="Non-interactive mode (skip prompts)")
    args = parser.parse_args()

    print()
    print("=== BMADder Bootstrap ===")
    print()

    # Step 1: Folder structure
    print("Step 1: Folder structure")
    run_script("init_bmadder.py")
    print()

    # Step 2: Rules and standards
    print("Step 2: Orchestrator + standards")
    run_script("create_rules.py")
    print()

    # Step 3: Sync headless skills for pipeline scripts
    print("Step 3: Headless skills")
    sync_script = ROOT / "scripts" / "sync_headless_skills.py"
    manifest = ROOT / "scripts" / "headless-skills" / "manifest.json"
    if sync_script.exists():
        if manifest.exists():
            # Check freshness first — only regenerate if stale
            try:
                result = subprocess.run(
                    [sys.executable, str(sync_script), "--check"],
                    capture_output=True, text=True, cwd=str(ROOT),
                )
                if result.returncode != 0:
                    print("  [STALE] Headless skills out of date, regenerating...")
                    run_script("sync_headless_skills.py")
                else:
                    print("  [OK]   Headless skills are up-to-date.")
            except Exception:
                print("  [WARN] Could not check staleness, regenerating...")
                run_script("sync_headless_skills.py")
        else:
            print("  [INIT] Generating headless skills for first time...")
            run_script("sync_headless_skills.py")
    else:
        print("  [SKIP] sync_headless_skills.py not found.")
    print()

    # Step 4: Config files
    print("Step 4: Config files")
    write_mise_toml()
    write_gitignore()
    make_executable()
    print()

    # Step 5: Verify tooling
    print("Step 5: Tooling check")
    tools_ok = True
    tools_ok &= check_tool("mise", ["mise", "--version"])
    tools_ok &= check_tool("uv", ["uv", "--version"])
    tools_ok &= check_tool("git", ["git", "--version"])

    # Optional tools (don't block on these)
    check_tool("claude", ["claude", "--version"])
    check_tool("codex", ["codex", "--version"])
    check_tool("gemini", ["gemini", "--version"])
    check_tool("cargo", ["cargo", "--version"])

    if not tools_ok:
        print()
        print("[WARN] Some required tools are missing. Install them:")
        print("  mise:  curl https://mise.run | sh")
        print("  uv:    mise use uv@latest (or: curl -LsSf https://astral.sh/uv/install.sh | sh)")
        print("  git:   apt install git (or your OS package manager)")
    print()

    # Step 6: Git init
    print("Step 6: Git repo")
    init_git()
    print()

    # Step 7: Check for PRD and architecture
    print("Step 7: Project files")
    prd = ROOT / "docs/prd.md"
    arch = ROOT / "docs/architecture.md"
    prd_ready = prd.exists() and prd.stat().st_size > 500
    arch_ready = arch.exists() and arch.stat().st_size > 500

    if prd_ready:
        print("  [OK]   docs/prd.md has content.")
    else:
        print("  [TODO] Fill in docs/prd.md with your product requirements.")

    if arch_ready:
        print("  [OK]   docs/architecture.md has content.")
    else:
        print("  [TODO] Fill in docs/architecture.md with your system design.")

    # Done
    print()
    print("=" * 50)
    if prd_ready and arch_ready:
        print("  BMADder is ready. Run the full cycle:")
        print("    ./scripts/bmadder.sh cycle")
    else:
        print("  BMADder structure is set up.")
        print("  Next: fill in PRD and architecture, then run:")
        print("    ./scripts/bmadder.sh cycle")
    print()
    print("  Other commands:")
    print("    ./scripts/bmadder.sh status    # show story states")
    print("    ./scripts/bmadder.sh plan      # SM + PO only")
    print("    ./scripts/bmadder.sh dev       # dev loop only")
    print("    ./scripts/bmadder.sh qa        # QA audit only")
    print("=" * 50)
    print()


if __name__ == "__main__":
    main()
