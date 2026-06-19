use bmadder_core::story::Story;

/// Return the context files for plan-phase SM invocation.
pub fn sm_batch_files(config: &bmadder_core::config::Config) -> Vec<String> {
    vec![
        config.paths.prd_file.to_string_lossy().to_string(),
        config.paths.architecture_file.to_string_lossy().to_string(),
    ]
}

/// Build SM batch prompt (bmadder plan — first phase).
/// Tells the skill what to do and provides the @files. The skill workflow
/// handles the mechanics.
pub fn sm_batch_prompt() -> String {
    r#"Bulk story sharding from the PRD into individual story files.

Context files provided: prd.md, architecture.md.

Pipeline rules:
- Stories go in: docs/backlog/stories/story-NNNN-slug.md
- Use YAML frontmatter with: status: "DRAFT", po_alignment: "PENDING"
- Each story MUST have sections: Context, Requirements, Acceptance Criteria, Implementation Notes, PO Alignment, QA Notes.

Pre-check:
BEFORE creating stories, list existing files in docs/backlog/stories/.
Do NOT recreate existing stories. SKIP stories with status: "READY_FOR_DEV" or "COMPLETED".
Only work on stories with status: "REVISE" or stories that don't exist yet.

Revision handling:
For stories with status: "REVISE":
1. Read ## PO Alignment for revision notes.
2. Address every issue. Update content. Set status: "DRAFT", po_alignment: "PENDING".
3. Append dated note under ## PO Alignment.

If no MISSING or REVISE stories remain, log that sharding is complete and exit.

Do NOT implement code. Do NOT approve stories.
Log a summary to _bmad/logs/activity.log.
"#
    .to_string()
}

/// Return the context files for plan-phase PO invocation.
pub fn po_batch_files(config: &bmadder_core::config::Config) -> Vec<String> {
    vec![
        config.paths.prd_file.to_string_lossy().to_string(),
        config.paths.architecture_file.to_string_lossy().to_string(),
    ]
}

/// Build PO batch prompt (bmadder plan — second phase).
pub fn po_batch_prompt() -> String {
    r#"Story quality review against the PRD and architecture.

Context files provided: prd.md, architecture.md.

Read EVERY story in docs/backlog/stories/ with status: "DRAFT".

For each draft story, evaluate against these criteria:
1. Does it map to at least one PRD requirement?
2. Is it consistent with the architecture?
3. Are Requirements and Acceptance Criteria clear, specific, testable?
4. Is scope small enough for one implementation + testing effort?
5. Are there dependency gaps (assumes work from a missing story)?

If ALL criteria pass:
- Set status: "READY_FOR_DEV", po_alignment: "APPROVED"
- Append dated approval note under ## PO Alignment

If ANY criterion fails:
- Set status: "REVISE", po_alignment: "REVISE"
- Append specific revision notes under ## PO Alignment

Log decisions to _bmad/logs/activity.log.
Do NOT move any story to IN_DEV or PENDING_QA.
"#
    .to_string()
}

/// Return the context files for dev-phase invocation.
pub fn dev_story_files(config: &bmadder_core::config::Config, story: &Story) -> Vec<String> {
    let mut files = vec![
        story
            .path
            .strip_prefix(&config.project_root)
            .unwrap_or(&story.path)
            .to_string_lossy()
            .to_string(),
        config.paths.architecture_file.to_string_lossy().to_string(),
    ];
    if config.paths.prd_file.exists() {
        files.push(config.paths.prd_file.to_string_lossy().to_string());
    }
    let progress = config.progress_file_path();
    if progress.exists() {
        files.push(
            progress
                .strip_prefix(&config.project_root)
                .unwrap_or(&progress)
                .to_string_lossy()
                .to_string(),
        );
    }
    files
}

/// Build Dev prompt for a single story.
pub fn dev_story_prompt(story: &Story) -> String {
    format!(
        r#"Implement story {story_id}: {title}

Context files provided: the story file, architecture, PRD, progress.

Rules:
- ONLY work on this story. Do not touch other stories.
- Do NOT skip feedback loops (build/test/lint).
- If you can't finish this iteration, commit partial progress, update progress.txt, and leave status "IN_DEV". Next iteration picks up.
- When build/test/lint pass AND all acceptance criteria are met:
  - Update story frontmatter: status: "PENDING_QA"
  - Fill in ## Implementation Notes: files changed, approach, decisions
- Append to _bmad/progress.txt: what you did, files modified, decisions, notes for QA
- Commit: `git add -A && git commit -m "feat({story_id}): <summary>"`
"#,
        story_id = story.frontmatter.story_id,
        title = story.frontmatter.title,
    )
}

/// Return the context files for QA-phase invocation.
pub fn qa_story_files(config: &bmadder_core::config::Config, story: &Story) -> Vec<String> {
    let mut files = vec![story
        .path
        .strip_prefix(&config.project_root)
        .unwrap_or(&story.path)
        .to_string_lossy()
        .to_string()];
    if config.paths.prd_file.exists() {
        files.push(config.paths.prd_file.to_string_lossy().to_string());
    }
    if config.paths.architecture_file.exists() {
        files.push(config.paths.architecture_file.to_string_lossy().to_string());
    }
    files
}

/// Build QA prompt for a single story.
pub fn qa_story_prompt(story: &Story) -> String {
    format!(
        r#"Audit story {story_id}: {title}

Context files provided: the story file, PRD, architecture.

Task:
1. Read the story's Requirements, Acceptance Criteria, Implementation Notes.
2. Review the code files referenced in Implementation Notes.
3. Run the test suite.
4. Verify each acceptance criterion against the implementation.
5. Check for regressions vs PRD and architecture.

If ALL checks pass:
- Update story: qa_status: "PASS", status: "COMPLETED"
- Append under ## QA Notes: what you tested, how, residual risks
- Do NOT run git commit (the orchestrator handles that)

If ANY check fails:
- Update story: qa_status: "FAIL", status: "REFIX"
- Append under ## QA Notes: what failed, steps to reproduce, fix guidance
- Do NOT commit

Log to _bmad/logs/activity.log.
"#,
        story_id = story.frontmatter.story_id,
        title = story.frontmatter.title,
    )
}

/// Return the context files for iterative single-story SM creation.
pub fn sm_single_files(config: &bmadder_core::config::Config) -> Vec<String> {
    let mut files = vec![
        config.paths.prd_file.to_string_lossy().to_string(),
        config.paths.architecture_file.to_string_lossy().to_string(),
    ];
    let progress = config.progress_file_path();
    if progress.exists() {
        files.push(
            progress
                .strip_prefix(&config.project_root)
                .unwrap_or(&progress)
                .to_string_lossy()
                .to_string(),
        );
    }
    files
}

/// Build SM single-story prompt for iterative mode (creates ONE story from PRD).
pub fn sm_single_prompt() -> String {
    r#"Create ONE story from the PRD.

Context files provided: prd.md, architecture.md, progress.txt.

Pre-work: Also run `git log --oneline -30` and review existing stories in docs/backlog/stories/.

Your task — pick exactly ONE:

A) If the PRD has features NOT yet implemented (no story file and not in progress.txt):
   → Create ONE story file following the workflow and checklist.
   → Respect dependencies: foundational/infrastructure stories first.
   → Filename: docs/backlog/stories/story-NNNN-<slug>.md (NNNN = next available 4-digit number)
   → Frontmatter must include: story_id, title, status: "DRAFT", po_alignment: "PENDING"
   → Log to _bmad/logs/activity.log.

B) If the PRD is FULLY implemented:
   → Append this exact line to _bmad/progress.txt:
      "ALL_DONE: PRD fully implemented."
   → Do NOT create any story file.

Create ONLY ONE story file. Do not implement code.
"#
    .to_string()
}

/// Return the context files for iterative SM write/revise.
pub fn sm_write_files(config: &bmadder_core::config::Config, story: &Story) -> Vec<String> {
    let mut files = vec![
        story
            .path
            .strip_prefix(&config.project_root)
            .unwrap_or(&story.path)
            .to_string_lossy()
            .to_string(),
        config.paths.prd_file.to_string_lossy().to_string(),
        config.paths.architecture_file.to_string_lossy().to_string(),
    ];
    let progress = config.progress_file_path();
    if progress.exists() {
        files.push(
            progress
                .strip_prefix(&config.project_root)
                .unwrap_or(&progress)
                .to_string_lossy()
                .to_string(),
        );
    }
    let activity = config.activity_log_path();
    if activity.exists() {
        files.push(
            activity
                .strip_prefix(&config.project_root)
                .unwrap_or(&activity)
                .to_string_lossy()
                .to_string(),
        );
    }
    files
}

/// Build SM write/revise prompt for iterative SM↔PO loop.
pub fn sm_write_story_prompt(_story: &Story) -> String {
    r#"Write or revise ONE story for the iterative pipeline.

Context files provided: the story file, prd.md, architecture.md, progress.txt, activity.log.

Your task (pick the correct one based on current story status):

A) If story status is "DRAFT" and content is mostly empty/template:
   → WRITE the full story following the workflow and checklist.
   → Set: status: "DRAFT", po_alignment: "PENDING"

B) If story status is "REVISE":
   → READ the ## PO Alignment section for revision notes.
   → Address EVERY issue raised. Update story content.
   → Set: status: "DRAFT", po_alignment: "PENDING"
   → Append dated note under ## PO Alignment: "SM revision: [summary of changes]"

Do NOT implement any code. Do NOT approve the story yourself.
Do NOT touch any other story files.
Log a brief summary to _bmad/logs/activity.log.
"#
    .to_string()
}

/// Return the context files for iterative single-story PO review.
pub fn po_single_files(config: &bmadder_core::config::Config, story: &Story) -> Vec<String> {
    let mut files = vec![
        story
            .path
            .strip_prefix(&config.project_root)
            .unwrap_or(&story.path)
            .to_string_lossy()
            .to_string(),
        config.paths.prd_file.to_string_lossy().to_string(),
        config.paths.architecture_file.to_string_lossy().to_string(),
    ];
    let progress = config.progress_file_path();
    if progress.exists() {
        files.push(
            progress
                .strip_prefix(&config.project_root)
                .unwrap_or(&progress)
                .to_string_lossy()
                .to_string(),
        );
    }
    files
}

/// Build PO single-story review prompt for iterative SM↔PO loop.
pub fn po_single_prompt(_story: &Story) -> String {
    r#"Review ONE story for the iterative pipeline.

Context files provided: the story file, prd.md, architecture.md, progress.txt.

Evaluate this story against these criteria:
1. Maps to at least one PRD requirement (no orphan work)
2. Consistent with the architecture (correct layers, patterns, naming)
3. Requirements are clear, specific, and unambiguous
4. Acceptance Criteria are numbered, testable, and specific (not vague)
5. Scope is right-sized: completable in one focused dev effort
6. Dependencies are explicit: any assumed prior work exists or is listed
7. agent_hint is set correctly
8. No duplicate scope with other COMPLETED or READY_FOR_DEV stories

Decision — you MUST pick exactly one:

IF ALL criteria are met:
  → Set story frontmatter: status: "READY_FOR_DEV", po_alignment: "APPROVED"
  → Append under ## PO Alignment: "$(date) PO APPROVED: [brief rationale]"

IF ANY criterion fails:
  → Set story frontmatter: status: "REVISE", po_alignment: "REVISE"
  → Append under ## PO Alignment: "$(date) PO REVISE: [numbered list of specific issues]"

Log your decision to _bmad/logs/activity.log. Do NOT implement code.
"#
    .to_string()
}
