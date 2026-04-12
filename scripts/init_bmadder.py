"""
init_bmadder.py — Creates the BMADder folder structure.
Safe to re-run; only creates directories that don't exist.

Usage: uv run scripts/init_bmadder.py
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def ensure_dir(path: Path):
    path.mkdir(parents=True, exist_ok=True)


def main():
    folders = [
        ROOT / "docs",
        ROOT / "docs/backlog",
        ROOT / "docs/backlog/epics",
        ROOT / "docs/backlog/stories",
        ROOT / "docs/standards",
        ROOT / "_bmad",
        ROOT / "_bmad/logs",
        ROOT / "src",
        ROOT / "scripts",
    ]
    for f in folders:
        ensure_dir(f)
        print(f"  [OK] {f.relative_to(ROOT)}/")

    # Seed empty PRD and architecture templates if they don't exist
    prd = ROOT / "docs/prd.md"
    if not prd.exists():
        prd.write_text("""# Product Requirements Document

## 1. Product Overview
<!-- What is this product? Who is it for? What problem does it solve? -->

## 2. Goals and Success Metrics
<!-- What does success look like? How will you measure it? -->

## 3. User Personas
<!-- Who are the primary users? -->

## 4. Functional Requirements
<!-- What must the product do? Be specific and testable. -->

## 5. Non-Functional Requirements
<!-- Performance, security, scalability, accessibility, etc. -->

## 6. Constraints and Assumptions
<!-- Technical constraints, budget, timeline, dependencies. -->

## 7. Out of Scope
<!-- What is explicitly NOT included in this version? -->
""", encoding="utf-8")
        print(f"  [OK] Created {prd.relative_to(ROOT)} (template)")
    else:
        print(f"  [SKIP] {prd.relative_to(ROOT)} already exists")

    arch = ROOT / "docs/architecture.md"
    if not arch.exists():
        arch.write_text("""# Architecture Document

## 1. System Overview
<!-- High-level description of the system and its components. -->

## 2. Technology Stack
<!-- Languages, frameworks, databases, infrastructure. -->

## 3. System Architecture
<!-- Component diagram, service boundaries, data flow. -->

## 4. Data Model
<!-- Core entities, relationships, database schema. -->

## 5. API Design
<!-- Endpoints, authentication, request/response formats. -->

## 6. Infrastructure
<!-- Deployment, CI/CD, monitoring, environments. -->

## 7. Security
<!-- Authentication, authorization, data protection. -->

## 8. Development Conventions
<!-- Code style, testing strategy, git workflow. -->
""", encoding="utf-8")
        print(f"  [OK] Created {arch.relative_to(ROOT)} (template)")
    else:
        print(f"  [SKIP] {arch.relative_to(ROOT)} already exists")

    print()
    print("BMADder structure ready.")
    print("Next: fill in docs/prd.md and docs/architecture.md, then run:")
    print("  uv run scripts/bootstrap_bmadder.py")


if __name__ == "__main__":
    main()
