use crate::story_io;
use bmadder_core::config::Config;
use bmadder_core::story::Story;

/// Build a guidance block describing available agent_hints for the SM.
fn agent_hints_guidance(config: &Config) -> String {
    if config.agent_hints.is_empty() {
        return String::new();
    }
    let mut g = String::from(
        "\nAgent model hints — set agent_hint in story frontmatter to route to a specific model:\n",
    );
    for (hint, model_key) in &config.agent_hints {
        let resolved = config
            .models
            .get(model_key)
            .cloned()
            .unwrap_or_else(|| model_key.clone());
        g.push_str(&format!(
            "  agent_hint: \"{}\" → model \"{}\"\n",
            hint, resolved
        ));
    }
    g.push_str("Omit agent_hint to use the default dev model.\n");
    g
}

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
pub fn sm_batch_prompt(config: &Config) -> String {
    let mut p = String::from(
        r#"Bulk story sharding from the PRD into individual story files.

Context files provided: prd.md, architecture.md.

Pipeline rules:
- Stories go in: docs/backlog/stories/story-NNNN-slug.md
- Frontmatter MUST include these exact fields (in this order):
    story_id: "STORY-NNNN"   ← must match the NNNN in the filename
    title: "..."
    status: "DRAFT"
    po_alignment: "PENDING"
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
"#,
    );
    p.push_str(&agent_hints_guidance(config));
    p
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
  - Add a ## File List section listing every file you changed (one per line, `- path/to/file`)
  - If the Acceptance Criteria section has checkbox items (`- [ ]`), check them all (`- [x]`)
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

Verdict (required):
End your QA Notes with exactly one line in the form `VERDICT: PASS` or `VERDICT: FAIL`.
When QA runs as a multi-model mixture, this verdict is read by a downstream
formatter that applies the PASS/FAIL decision to the story frontmatter.

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

/// Build a one-line-per-story snapshot of docs/backlog/stories/ for the SM.
///
/// The stories folder is the source of truth for "what exists" — this lets the
/// SM pick the next NNNN, avoid duplicating existing stories, and decide
/// A (create next) vs B (all done) from real state. It also means "delete the
/// stories folder" is a complete from-scratch reset: an empty listing ⇒ no
/// story covers any PRD feature ⇒ path A.
fn existing_stories_listing(config: &Config) -> String {
    let paths = match story_io::list_stories(&config.paths.stories_dir) {
        Ok(p) => p,
        Err(_) => return "(could not list docs/backlog/stories/)".to_string(),
    };
    if paths.is_empty() {
        return "(none — no stories exist yet; this is a from-scratch run)".to_string();
    }
    let mut rows = Vec::with_capacity(paths.len());
    for path in &paths {
        let fname = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let (sid, title, status) = match story_io::parse_story_file(path) {
            Ok(s) => (
                s.frontmatter.story_id.clone(),
                s.frontmatter.title.clone(),
                s.frontmatter.status.label().to_string(),
            ),
            Err(_) => (
                "?".to_string(),
                "(unparseable)".to_string(),
                "?".to_string(),
            ),
        };
        rows.push(format!(
            "- {fname} | {sid} | \"{title}\" | {status}",
            fname = fname,
            sid = sid,
            title = title,
            status = status
        ));
    }
    rows.join("\n")
}

/// Build SM single-story prompt for iterative mode (creates ONE story from PRD).
pub fn sm_single_prompt(config: &Config) -> String {
    let listing = existing_stories_listing(config);
    let p = format!(
        r#"Create ONE story from the PRD.

The full contents of the context files (prd.md, architecture.md, progress.txt if present) are included in your context. You do NOT need to read them from disk — work directly from the provided contents.

## Existing Stories
The current contents of docs/backlog/stories/ (authoritative — this is the source of truth for what already exists):
{listing}

Use that listing to decide what to do next:
- The next story number NNNN = the highest existing story number + 1 (or 0001 if the listing is empty).
- Do NOT create a story that duplicates an existing one (same feature / overlapping scope).

Your task — pick exactly ONE:

A) If the PRD has features NOT yet covered by an existing story (especially features with no READY_FOR_DEV or COMPLETED story for them):
   → Create ONE story file following the workflow and checklist.
   → Respect dependencies: foundational/infrastructure stories first.
   → Filename: docs/backlog/stories/story-NNNN-<slug>.md (NNNN = next available 4-digit number from the listing above)
   → Frontmatter must include: story_id, title, status: "DRAFT", po_alignment: "PENDING"
   → Log to _bmad/logs/activity.log.

B) If EVERY PRD feature already has a READY_FOR_DEV or COMPLETED story in the listing above:
   → Append this exact line to _bmad/progress.txt:
      "ALL_DONE: PRD fully implemented."
   → Do NOT create any story file.

Produce the deliverable directly. Do NOT refuse on the grounds that you cannot access the filesystem — the inputs you need are already in your context, including the existing-stories listing above. Create ONLY ONE story file. Do not implement code.
        "#,
        listing = listing,
    );
    let mut p = p;
    p.push_str(&agent_hints_guidance(config));
    p
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
pub fn sm_write_story_prompt(config: &Config, _story: &Story) -> String {
    let mut p = String::from(
        r#"Write or revise ONE story for the iterative pipeline.

The full contents of the context files (the story file, prd.md, architecture.md, progress.txt, activity.log) are included in your context. You do NOT need to read them from disk — work directly from the provided contents, including the story's current ## PO Alignment section.

Your task (pick the correct one based on current story status):

A) If story status is "DRAFT" and content is mostly empty/template:
   → WRITE the full story following the workflow and checklist.
   → Set: status: "DRAFT", po_alignment: "PENDING"

B) If story status is "REVISE":
   → The story's ## PO Alignment section (with the PO's revision notes) is in your context.
   → Address EVERY issue raised there. Update story content.
   → Set: status: "DRAFT", po_alignment: "PENDING"
   → Append dated note under ## PO Alignment: "SM revision: [summary of changes]"

Produce the deliverable directly. Do NOT refuse on the grounds that you cannot access the filesystem — the inputs you need are already in your context. Do NOT implement any code. Do NOT approve the story yourself. Do NOT touch any other story files. Log a brief summary to _bmad/logs/activity.log.
"#,
    );
    p.push_str(&agent_hints_guidance(config));
    p
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with_stories(stories_dir: &std::path::Path) -> Config {
        let root = stories_dir.parent().unwrap().to_path_buf();
        let toml_path = root.join("bmadder.toml");
        std::fs::write(
            &toml_path,
            format!("[paths]\nstories_dir = \"{}\"\n", stories_dir.display()),
        )
        .unwrap();
        Config::load(&toml_path).unwrap()
    }

    fn write_story(dir: &std::path::Path, fname: &str, sid: &str, title: &str, status: &str) {
        std::fs::write(
            dir.join(fname),
            format!(
                "---\nstory_id: \"{sid}\"\ntitle: \"{title}\"\nstatus: \"{status}\"\npo_alignment: \"PENDING\"\n---\n# {title}\n",
                sid = sid,
                title = title,
                status = status
            ),
        )
        .unwrap();
    }

    #[test]
    fn existing_stories_listing_empty_when_no_stories() {
        let dir = tempfile::tempdir().unwrap();
        let stories = dir.path().join("stories");
        std::fs::create_dir_all(&stories).unwrap();
        let config = cfg_with_stories(&stories);
        let listing = existing_stories_listing(&config);
        assert!(listing.contains("none"), "empty dir: {}", listing);
        assert!(!listing.contains("story-"));
    }

    #[test]
    fn existing_stories_listing_empty_when_dir_missing() {
        // A missing stories dir behaves like an empty one (from-scratch reset).
        let dir = tempfile::tempdir().unwrap();
        let stories = dir.path().join("stories"); // not created
        let config = cfg_with_stories(&stories);
        let listing = existing_stories_listing(&config);
        assert!(listing.contains("none"), "missing dir: {}", listing);
    }

    #[test]
    fn existing_stories_listing_shows_existing_story_row() {
        let dir = tempfile::tempdir().unwrap();
        let stories = dir.path().join("stories");
        std::fs::create_dir_all(&stories).unwrap();
        write_story(
            &stories,
            "story-0001-db-schema.md",
            "0001",
            "Database schema",
            "READY_FOR_DEV",
        );
        let config = cfg_with_stories(&stories);
        let listing = existing_stories_listing(&config);
        assert!(listing.contains("story-0001-db-schema.md"), "{}", listing);
        assert!(listing.contains("0001"), "{}", listing);
        assert!(listing.contains("Database schema"), "{}", listing);
        assert!(listing.contains("READY_FOR_DEV"), "{}", listing);
    }

    #[test]
    fn existing_stories_listing_survives_unparseable_story() {
        let dir = tempfile::tempdir().unwrap();
        let stories = dir.path().join("stories");
        std::fs::create_dir_all(&stories).unwrap();
        std::fs::write(
            stories.join("story-0099-broken.md"),
            "not valid frontmatter",
        )
        .unwrap();
        let config = cfg_with_stories(&stories);
        let listing = existing_stories_listing(&config);
        // Falls back to a row with placeholders rather than panicking.
        assert!(listing.contains("story-0099-broken.md"), "{}", listing);
        assert!(listing.contains("unparseable"), "{}", listing);
    }

    #[test]
    fn sm_single_prompt_embeds_listing_and_next_number_guidance() {
        let dir = tempfile::tempdir().unwrap();
        let stories = dir.path().join("stories");
        std::fs::create_dir_all(&stories).unwrap();
        write_story(
            &stories,
            "story-0001-db-schema.md",
            "0001",
            "Database schema",
            "READY_FOR_DEV",
        );
        let config = cfg_with_stories(&stories);
        let prompt = sm_single_prompt(&config);
        assert!(prompt.contains("## Existing Stories"), "{}", prompt);
        assert!(prompt.contains("story-0001-db-schema.md"), "{}", prompt);
        assert!(prompt.contains("next story number NNNN"), "{}", prompt);
        assert!(
            prompt.contains("Do NOT create a story that duplicates"),
            "{}",
            prompt
        );
    }
}
