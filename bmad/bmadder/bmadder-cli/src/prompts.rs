use bmadder_core::story::Story;

/// Build SM batch prompt (bmadder plan — first phase).
pub fn sm_batch_prompt(_stories_exist: bool) -> String {
    let mut p = String::new();
    p.push_str("You are the Scrum Master running in AUTOMATED PIPELINE mode (non-interactive, no user input).\n\n");
    p.push_str("Follow the consolidated headless skill instructions:\n");
    p.push_str("@scripts/headless-skills/sm-create-stories.md\n\n");
    p.push_str("Context documents:\n");
    p.push_str("@docs/prd.md\n");
    p.push_str("@docs/architecture.md\n\n");
    p.push_str("Pipeline rules:\n");
    p.push_str("- Stories go in: docs/backlog/stories/story-NNNN-slug.md\n");
    p.push_str("- Use YAML frontmatter with: status: \"DRAFT\", po_alignment: \"PENDING\"\n");
    p.push_str("- Each story MUST have sections: Context, Requirements, Acceptance Criteria,\n");
    p.push_str("  Implementation Notes, PO Alignment, QA Notes.\n\n");
    p.push_str("Pre-check:\n");
    p.push_str("BEFORE creating stories, list existing files in docs/backlog/stories/.\n");
    p.push_str("Do NOT recreate existing stories. SKIP stories with status: \"READY_FOR_DEV\" or \"COMPLETED\".\n");
    p.push_str("Only work on stories with status: \"REVISE\" or stories that don't exist yet.\n\n");
    p.push_str("Revision handling:\n");
    p.push_str("For stories with status: \"REVISE\":\n");
    p.push_str("1. Read ## PO Alignment for revision notes.\n");
    p.push_str("2. Address every issue. Update content. Set status: \"DRAFT\", po_alignment: \"PENDING\".\n");
    p.push_str("3. Append dated note under ## PO Alignment.\n\n");
    p.push_str(
        "If no MISSING or REVISE stories remain, log that sharding is complete and exit.\n\n",
    );
    p.push_str("Do NOT implement code. Do NOT approve stories.\n");
    p.push_str("Log a summary to _bmad/logs/activity.log.\n");
    p
}

/// Build PO batch prompt (bmadder plan — second phase).
pub fn po_batch_prompt() -> String {
    let mut p = String::new();
    p.push_str("You are the Product Owner running in AUTOMATED PIPELINE mode (non-interactive, no user input).\n\n");
    p.push_str("Follow the story quality checklist:\n");
    p.push_str("@scripts/headless-skills/po-review.md\n\n");
    p.push_str("Context documents:\n");
    p.push_str("@docs/prd.md\n");
    p.push_str("@docs/architecture.md\n\n");
    p.push_str("Read EVERY story in docs/backlog/stories/ with status: \"DRAFT\".\n\n");
    p.push_str("For each draft story, evaluate against the checklist criteria above PLUS:\n");
    p.push_str("1. Does it map to at least one PRD requirement?\n");
    p.push_str("2. Is it consistent with the architecture?\n");
    p.push_str("3. Are Requirements and Acceptance Criteria clear, specific, testable?\n");
    p.push_str("4. Is scope small enough for one implementation + testing effort?\n");
    p.push_str("5. Are there dependency gaps (assumes work from a missing story)?\n\n");
    p.push_str("If ALL criteria pass:\n");
    p.push_str("- Set status: \"READY_FOR_DEV\", po_alignment: \"APPROVED\"\n");
    p.push_str("- Append dated approval note under ## PO Alignment\n\n");
    p.push_str("If ANY criterion fails:\n");
    p.push_str("- Set status: \"REVISE\", po_alignment: \"REVISE\"\n");
    p.push_str("- Append specific revision notes under ## PO Alignment\n\n");
    p.push_str("Log decisions to _bmad/logs/activity.log.\n");
    p.push_str("Do NOT move any story to IN_DEV or PENDING_QA.\n");
    p
}

/// Build Dev prompt for a single story.
pub fn dev_story_prompt(story: &Story) -> String {
    let mut p = String::new();
    p.push_str("You are the Developer running in AUTOMATED PIPELINE mode (non-interactive, no user input).\n\n");
    p.push_str("Follow the consolidated dev workflow:\n");
    p.push_str("@scripts/headless-skills/dev-story.md\n\n");
    p.push_str(&format!(
        "Working on ONE story:\n  ID: {}\n  File: @{}\n\n",
        story.frontmatter.story_id,
        story.path.display()
    ));
    p.push_str("Context:\n");
    p.push_str("@docs/architecture.md\n");
    p.push_str("@docs/prd.md\n");
    p.push_str("@_bmad/progress.txt\n\n");
    p.push_str("Also run: `git log --oneline -20` to see what previous iterations built.\n\n");
    p.push_str("Completion criteria:\n");
    p.push_str("- When build/test/lint pass AND all acceptance criteria are met:\n");
    p.push_str("  - Update story frontmatter: status: \"PENDING_QA\"\n");
    p.push_str("  - Fill in ## Implementation Notes: files changed, approach, decisions\n");
    p.push_str(
        "- Append to _bmad/progress.txt: what you did, files modified, decisions, notes for QA\n",
    );
    p.push_str("- Commit: `git add -A && git commit -m \"feat(STORY-NNNN): <summary>\"`\n\n");
    p.push_str("Rules:\n");
    p.push_str("- ONLY work on this story. Do not touch other stories.\n");
    p.push_str("- Do NOT skip feedback loops.\n");
    p.push_str("- If you can't finish this iteration, commit partial progress, update\n");
    p.push_str("  progress.txt, and leave status \"IN_DEV\". Next iteration picks up.\n");
    p
}

/// Build QA prompt for a single story.
pub fn qa_story_prompt(story: &Story) -> String {
    let mut p = String::new();
    p.push_str("You are the QA Auditor running in AUTOMATED PIPELINE mode (non-interactive, no user input).\n\n");
    p.push_str("Follow the consolidated code review workflow:\n");
    p.push_str("@scripts/headless-skills/qa-review.md\n\n");
    p.push_str(&format!(
        "Auditing ONE story:\n  ID: {}\n  File: @{}\n\n",
        story.frontmatter.story_id,
        story.path.display()
    ));
    p.push_str("Context:\n");
    p.push_str("@docs/prd.md\n");
    p.push_str("@docs/architecture.md\n\n");
    p.push_str("Task:\n");
    p.push_str("1. Read the story's Requirements, Acceptance Criteria, Implementation Notes.\n");
    p.push_str("2. Review the code files referenced in Implementation Notes.\n");
    p.push_str("3. Run the test suite.\n");
    p.push_str("4. Verify each acceptance criterion against the implementation.\n");
    p.push_str("5. Check for regressions vs PRD and architecture.\n\n");
    p.push_str("If ALL checks pass:\n");
    p.push_str("- Update story: qa_status: \"PASS\", status: \"COMPLETED\"\n");
    p.push_str("- Append under ## QA Notes: what you tested, how, residual risks\n");
    p.push_str("- Do NOT run git commit (the orchestrator handles that)\n\n");
    p.push_str("If ANY check fails:\n");
    p.push_str("- Update story: qa_status: \"FAIL\", status: \"REFIX\"\n");
    p.push_str("- Append under ## QA Notes: what failed, steps to reproduce, fix guidance\n");
    p.push_str("- Do NOT commit\n\n");
    p.push_str("Log to _bmad/logs/activity.log.\n");
    p
}

/// Build SM single-story prompt for iterative mode (creates ONE story from PRD).
pub fn sm_single_prompt() -> String {
    let mut p = String::new();
    p.push_str("You are the Scrum Master running in AUTOMATED PIPELINE mode (non-interactive, no user input).\n\n");
    p.push_str("Follow the consolidated headless skill instructions:\n");
    p.push_str("@scripts/headless-skills/sm-create-story.md\n\n");
    p.push_str("Context documents:\n");
    p.push_str("@docs/prd.md\n");
    p.push_str("@docs/architecture.md\n");
    p.push_str("@_bmad/progress.txt\n\n");
    p.push_str("Also run: `git log --oneline -30`\n");
    p.push_str("And review existing stories in: docs/backlog/stories/\n\n");
    p.push_str("Your task -- pick exactly ONE:\n\n");
    p.push_str(
        "A) If the PRD has features NOT yet implemented (no story file and not in progress.txt):\n",
    );
    p.push_str("   -> Create ONE story file following the skill workflow and checklist.\n");
    p.push_str("   -> Respect dependencies: foundational/infrastructure stories first.\n");
    p.push_str("   -> Filename: docs/backlog/stories/story-NNNN-<slug>.md\n");
    p.push_str("      (NNNN = next available 4-digit number)\n");
    p.push_str("   -> Frontmatter must include: story_id, title, status: \"DRAFT\", po_alignment: \"PENDING\"\n");
    p.push_str("   -> Log to _bmad/logs/activity.log.\n\n");
    p.push_str("B) If the PRD is FULLY implemented:\n");
    p.push_str("   -> Append this exact line to _bmad/progress.txt:\n");
    p.push_str("       \"ALL_DONE: PRD fully implemented.\"\n");
    p.push_str("   -> Do NOT create any story file.\n\n");
    p.push_str("Create ONLY ONE story file. Do not implement code.\n");
    p
}

/// Build SM write/revise prompt for iterative SM↔PO loop.
pub fn sm_write_story_prompt(story: &Story) -> String {
    let mut p = String::new();
    p.push_str("You are the Scrum Master running in AUTOMATED PIPELINE mode (non-interactive, no user input).\n\n");
    p.push_str("Follow the consolidated headless skill instructions:\n");
    p.push_str("@scripts/headless-skills/sm-create-story.md\n\n");
    p.push_str(&format!(
        "You are working on ONE story for the iterative pipeline:\n  Story ID: {}\n  File: @{}\n\n",
        story.frontmatter.story_id,
        story.path.display()
    ));
    p.push_str("Context documents:\n");
    p.push_str("@docs/prd.md\n");
    p.push_str("@docs/architecture.md\n");
    p.push_str("@_bmad/progress.txt\n");
    p.push_str("@_bmad/logs/activity.log\n\n");
    p.push_str("Your task (pick the correct one based on current story status):\n\n");
    p.push_str("A) If story status is \"DRAFT\" and content is mostly empty/template:\n");
    p.push_str("   -> WRITE the full story following the workflow and checklist.\n");
    p.push_str("   -> Set: status: \"DRAFT\", po_alignment: \"PENDING\"\n\n");
    p.push_str("B) If story status is \"REVISE\":\n");
    p.push_str("   -> READ the ## PO Alignment section for revision notes.\n");
    p.push_str("   -> Address EVERY issue raised. Update story content.\n");
    p.push_str("   -> Set: status: \"DRAFT\", po_alignment: \"PENDING\"\n");
    p.push_str(
        "   -> Append dated note under ## PO Alignment: \"SM revision: [summary of changes]\"\n\n",
    );
    p.push_str("Do NOT implement any code. Do NOT approve the story yourself.\n");
    p.push_str("Do NOT touch any other story files.\n");
    p.push_str("Log a brief summary to _bmad/logs/activity.log.\n");
    p
}

/// Build PO single-story review prompt for iterative SM↔PO loop.
pub fn po_single_prompt(story: &Story) -> String {
    let mut p = String::new();
    p.push_str("You are the Product Owner running in AUTOMATED PIPELINE mode (non-interactive, no user input).\n\n");
    p.push_str("Follow the story quality checklist:\n");
    p.push_str("@scripts/headless-skills/po-review.md\n\n");
    p.push_str(&format!(
        "You are reviewing ONE story for the iterative pipeline:\n  Story ID: {}\n  File: @{}\n\n",
        story.frontmatter.story_id,
        story.path.display()
    ));
    p.push_str("Context documents:\n");
    p.push_str("@docs/prd.md\n");
    p.push_str("@docs/architecture.md\n");
    p.push_str("@_bmad/progress.txt\n\n");
    p.push_str("Evaluate this story against the checklist criteria above PLUS:\n");
    p.push_str("1. Maps to at least one PRD requirement (no orphan work)\n");
    p.push_str("2. Consistent with the architecture (correct layers, patterns, naming)\n");
    p.push_str("3. Requirements are clear, specific, and unambiguous\n");
    p.push_str("4. Acceptance Criteria are numbered, testable, and specific (not vague)\n");
    p.push_str("5. Scope is right-sized: completable in one focused dev effort\n");
    p.push_str("6. Dependencies are explicit: any assumed prior work exists or is listed\n");
    p.push_str("7. agent_hint is set correctly\n");
    p.push_str("8. No duplicate scope with other COMPLETED or READY_FOR_DEV stories\n\n");
    p.push_str("Decision — you MUST pick exactly one:\n\n");
    p.push_str("IF ALL criteria are met:\n");
    p.push_str(
        "  -> Set story frontmatter: status: \"READY_FOR_DEV\", po_alignment: \"APPROVED\"\n",
    );
    p.push_str("  -> Append under ## PO Alignment: \"$(date) PO APPROVED: [brief rationale]\"\n\n");
    p.push_str("IF ANY criterion fails:\n");
    p.push_str("  -> Set story frontmatter: status: \"REVISE\", po_alignment: \"REVISE\"\n");
    p.push_str("  -> Append under ## PO Alignment: \"$(date) PO REVISE: [numbered list of specific issues]\"\n\n");
    p.push_str("Log your decision to _bmad/logs/activity.log. Do NOT implement code.\n");
    p
}
