//! Shared helpers for the moa-rust two-phase pattern.
//!
//! moa-rust writes a multi-model consensus document to `output/moa-*.md` but
//! does not modify story files directly (its backends run without file tools).
//! bmadder follows each moa-rust invocation with a `pi` pass that reads the
//! consensus and applies the structured decision to the story file(s).
//!
//! This module hosts every consensus-apply pass:
//! - Plan SM: `format_consensus_as_story` (iterative, one story) and
//!   `format_consensus_into_stories` (batch, all stories from one PRD shard).
//! - Plan PO: `apply_po_consensus` (iterative, one story) and
//!   `apply_po_consensus_batch` (batch, all DRAFT stories).
//! - QA: `apply_qa_consensus` (one story, used by both batch and iterative).

use std::path::{Path, PathBuf};

use bmadder_core::config::{Config, Phase};
use bmadder_core::story::Story;

use crate::agent::invoke_agent;
use crate::logging;
use crate::story_io;

/// Find the latest moa-rust consensus output in the project's `output/` directory
/// that was written AFTER `newer_than`.
///
/// moa-rust writes files named `moa-YYYYMMDD-HHMMSS-<hash>.md` (or
/// `spec-moa-<slug>.md` in bmad-compatible mode). Returns the most recently
/// modified `moa-*.md` file whose mtime is strictly newer than `newer_than`,
/// or `None` if the directory is empty/missing/no file qualifies.
///
/// The `newer_than` gate is load-bearing: callers capture `SystemTime::now()`
/// just before invoking moa-rust and pass it here. Without it, a failed moa-rust
/// run (no new file written) would make this return an EARLIER phase's consensus
/// — e.g. an SM consensus applied as a QA verdict — manufacturing the exact
/// ambiguous statuses the per-story circuit breaker guards. See incident report.
pub fn find_latest_moa_output(
    config: &Config,
    newer_than: std::time::SystemTime,
) -> Option<PathBuf> {
    let output_dir = config.project_root.join("output");
    if !output_dir.exists() {
        return None;
    }
    let mut moa_files: Vec<PathBuf> = std::fs::read_dir(&output_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("moa-") && n.ends_with(".md"))
                .unwrap_or(false)
        })
        .collect();
    moa_files.sort_by(|a, b| {
        b.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(
                &a.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            )
    });
    // Only accept a consensus written after the invocation that produced it
    // started. Stale files (from an earlier phase / failed run) are skipped.
    moa_files.into_iter().find(|p| {
        p.metadata()
            .and_then(|m| m.modified())
            .map(|t| t > newer_than)
            .unwrap_or(false)
    })
}

/// Run `pi` to convert a moa-rust consensus document into a SINGLE properly
/// formatted story file (iterative SM mode — one story per consensus).
pub fn format_consensus_as_story(
    config: &Config,
    consensus_path: &Path,
    context_files: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let consensus_rel = rel_path(config, consensus_path);

    let existing = story_io::list_stories(&config.paths.stories_dir)?;
    let next_num = next_story_num(&existing);

    let format_prompt = format!(
        r#"You are the Scrum Master formatting a consensus document into a proper story file.

A multi-model consensus has been generated. Read it and write a SINGLE properly formatted story file.

Consensus document: @{consensus}

Rules:
- Write the story file to: docs/backlog/stories/story-{nnnn:04}-<slug>.md
- YAML frontmatter MUST include:
    story_id: "STORY-{nnnn:04}"
    title: "..."
    status: "DRAFT"
    po_alignment: "PENDING"
    agent_hint: "specialist" (or "generalist" / "planning-qa" based on the work type)
- Story sections MUST include: Context, Requirements, Acceptance Criteria, Implementation Notes, PO Alignment, QA Notes, Tasks
- Extract the BEST consensus recommendation from the document — don't include the deliberation, just the final decisions
- Acceptance Criteria must be numbered, specific, and testable (Given/When/Then where possible)
- Log to _bmad/logs/activity.log
"#,
        consensus = consensus_rel,
        nnnn = next_num,
    );

    run_apply_pass(
        config,
        "sm",
        Phase::Plan,
        &consensus_rel,
        context_files,
        &format_prompt,
        "format consensus into story file",
    )
}

/// Run `pi` to revise an EXISTING story file IN PLACE from a moa-rust SM
/// consensus (iterative SM write/revise mode).
///
/// This is the revise-flow counterpart to `format_consensus_as_story`
/// (which creates a new file). moa-rust only emits a consensus document —
/// it has no file-edit tools, so without this apply pass the SM step would
/// leave the story untouched and the SM↔PO loop would spin forever
/// (PO keeps appending "PO REVISE" blocks to a story whose Requirements/AC
/// never change). This pass hands the consensus to `pi` (which can edit
/// files) and rewrites the story body to reflect the consensus and address
/// every PO REVISE item, then resets the story to DRAFT/PENDING for re-review.
pub fn apply_sm_consensus(
    config: &Config,
    consensus_path: &Path,
    story: &Story,
    context_files: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let consensus_rel = rel_path(config, consensus_path);
    let story_rel = rel_path(config, &story.path);

    let format_prompt = format!(
        r#"You are the Scrum Master revising an EXISTING story file in place from a multi-model consensus.

A multi-model consensus has been generated for this story. Read it and rewrite the story file's content to reflect the consensus decisions.

Consensus document: @{consensus}
Story file to revise IN PLACE (overwrite this file): @{story}

Rules:
- REWRITE the existing story file's body sections — Context, Requirements, Acceptance Criteria, Implementation Notes, Tasks — to reflect the consensus and to address EVERY issue listed under ## PO Alignment (the PO REVISE items from the previous review). Do not just append a note; update the actual content.
- Preserve the YAML frontmatter story_id and title exactly. Set status: "DRAFT" and po_alignment: "PENDING" so the PO re-reviews the revised story.
- Keep the ## PO Alignment and ## QA Notes sections present (do not delete prior review history), but the content sections above them must change.
- Acceptance Criteria must be numbered, specific, and testable (Given/When/Then where possible).
- Do NOT create a new story file. Do NOT touch other story files. Do NOT implement code — this is story authoring, not development.
- Log to _bmad/logs/activity.log.
"#,
        consensus = consensus_rel,
        story = story_rel,
    );
    run_apply_pass(
        config,
        "sm",
        Phase::Plan,
        &consensus_rel,
        context_files,
        &format_prompt,
        "revise story from SM consensus",
    )?;
    logging::log_event(
        config,
        &logging::StoryEvent::simple(
            "SM",
            &story.frontmatter.story_id,
            "SM_CONSENSUS_APPLIED",
            "SM consensus applied — story body revised in place",
        ),
    );
    Ok(())
}

/// Run `pi` to convert a moa-rust consensus document into MULTIPLE story files
/// (batch SM mode — one consensus containing the full PRD sharding).
///
/// Unlike `format_consensus_as_story`, the consensus may describe several
/// stories; pi writes one file per story, numbering sequentially from the
/// highest existing story id.
pub fn format_consensus_into_stories(
    config: &Config,
    consensus_path: &Path,
    context_files: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let consensus_rel = rel_path(config, consensus_path);

    let existing = story_io::list_stories(&config.paths.stories_dir)?;
    let next_num = next_story_num(&existing);

    let format_prompt = format!(
        r#"You are the Scrum Master formatting a consensus document into properly formatted story files.

A multi-model consensus has been generated that shards the PRD into multiple stories. Read it and write ONE story file PER story described in the consensus.

Consensus document: @{consensus}

Rules:
- Write each story file to: docs/backlog/stories/story-{nnnn:04}-<slug>.md
  (start numbering at {nnnn:04}, incrementing per story)
- YAML frontmatter for EACH story MUST include:
    story_id: "STORY-{nnnn:04}"
    title: "..."
    status: "DRAFT"
    po_alignment: "PENDING"
    agent_hint: "specialist" (or "generalist" / "planning-qa" based on the work type)
- Each story MUST include sections: Context, Requirements, Acceptance Criteria, Implementation Notes, PO Alignment, QA Notes, Tasks
- Extract the BEST consensus recommendation per story — don't include the deliberation, just the final decisions
- Acceptance Criteria must be numbered, specific, and testable (Given/When/Then where possible)
- Do NOT recreate stories that already exist in docs/backlog/stories/. Skip READY_FOR_DEV or COMPLETED stories.
- Log to _bmad/logs/activity.log
"#,
        consensus = consensus_rel,
        nnnn = next_num,
    );

    run_apply_pass(
        config,
        "sm",
        Phase::Plan,
        &consensus_rel,
        context_files,
        &format_prompt,
        "format consensus into story files",
    )
}

/// Run `pi` to apply a moa-rust PO review consensus to a SINGLE story file
/// (iterative PO mode). Reads APPROVE → READY_FOR_DEV, or REVISE → REVISE.
pub fn apply_po_consensus(
    config: &Config,
    consensus_path: &Path,
    story: &Story,
    context_files: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let consensus_rel = rel_path(config, consensus_path);
    let story_rel = rel_path(config, &story.path);

    let format_prompt = format!(
        r#"You are the Product Owner applying a consensus review decision to a story file.

A multi-model consensus has been generated for this story. Read it and apply the decision.

Consensus document: @{consensus}
Story file: @{story}

Rules:
- Read the consensus decision: APPROVE or REVISE
- If APPROVED: update story frontmatter to status: "READY_FOR_DEV", po_alignment: "APPROVED"
  - Append under ## PO Alignment: "PO APPROVED: [brief rationale from consensus]"
- If REVISE: update story frontmatter to status: "REVISE", po_alignment: "REVISE"
  - Append under ## PO Alignment: "PO REVISE: [numbered list of issues from consensus]"
- Do NOT implement any code. Do NOT touch other story files.
- Log to _bmad/logs/activity.log.
"#,
        consensus = consensus_rel,
        story = story_rel,
    );
    run_apply_pass(
        config,
        "po",
        Phase::Plan,
        &consensus_rel,
        context_files,
        &format_prompt,
        "apply PO consensus to story file",
    )?;
    logging::log_event(
        config,
        &logging::StoryEvent::simple(
            "PO",
            &story.frontmatter.story_id,
            "PO_CONSENSUS_APPLIED",
            "PO consensus applied (APPROVE → READY_FOR_DEV or REVISE)",
        ),
    );
    Ok(())
}

/// Run `pi` to apply a moa-rust PO review consensus to ALL DRAFT stories
/// (batch PO mode). The consensus contains review decisions for every draft
/// story; pi reads it and updates each story file's status accordingly.
pub fn apply_po_consensus_batch(
    config: &Config,
    consensus_path: &Path,
    context_files: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let consensus_rel = rel_path(config, consensus_path);

    let format_prompt = format!(
        r#"You are the Product Owner applying a batch consensus review to all DRAFT stories.

A multi-model consensus has been generated that reviews EVERY DRAFT story in docs/backlog/stories/. Read the consensus and apply each decision to the matching story file.

Consensus document: @{consensus}

Rules:
- For EACH DRAFT story referenced in the consensus:
  - If the consensus APPROVES that story:
    - Update its frontmatter to status: "READY_FOR_DEV", po_alignment: "APPROVED"
    - Append under ## PO Alignment: "PO APPROVED: [brief rationale from consensus]"
  - If the consensus says REVISE:
    - Update its frontmatter to status: "REVISE", po_alignment: "REVISE"
    - Append under ## PO Alignment: "PO REVISE: [numbered list of issues from consensus]"
- Match consensus sections to story files by story_id or title. Skip stories that are not DRAFT.
- Do NOT implement any code. Do NOT create new stories. Do NOT touch READY_FOR_DEV/COMPLETED stories.
- Log to _bmad/logs/activity.log.
"#,
        consensus = consensus_rel,
    );

    run_apply_pass(
        config,
        "po",
        Phase::Plan,
        &consensus_rel,
        context_files,
        &format_prompt,
        "apply PO consensus batch to DRAFT stories",
    )
}

/// Run `pi` to apply a moa-rust QA consensus to a SINGLE story file.
///
/// Reads the consensus verdict (PASS → COMPLETED, or FAIL → REFIX) and updates
/// the story frontmatter + `## QA Notes` section. Used by both batch and
/// iterative QA phases.
pub fn apply_qa_consensus(
    config: &Config,
    consensus_path: &Path,
    story: &Story,
    context_files: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let consensus_rel = rel_path(config, consensus_path);
    let story_rel = rel_path(config, &story.path);

    let format_prompt = format!(
        r#"You are the QA reviewer applying a multi-model consensus decision to a story file.

A multi-model consensus has been generated for this story. Read it and apply the decision.

Consensus document: @{consensus}
Story file: @{story}

Rules:
- Read the consensus verdict: PASS or FAIL
- If the consensus says ALL acceptance criteria pass:
  - Update story frontmatter: qa_status: "PASS", status: "COMPLETED"
  - Append under ## QA Notes: "QA PASS: [brief rationale from consensus, citing which models agreed]"
- If the consensus says ANY criterion fails:
  - Update story frontmatter: qa_status: "FAIL", status: "REFIX"
  - Append under ## QA Notes: "QA FAIL: [numbered list of failures + fix guidance from consensus]"
- Do NOT run git commit (the orchestrator handles that).
- Do NOT touch other story files.
- Log to _bmad/logs/activity.log.
"#,
        consensus = consensus_rel,
        story = story_rel,
    );

    run_apply_pass(
        config,
        "qa",
        Phase::QA,
        &consensus_rel,
        context_files,
        &format_prompt,
        "apply QA consensus to story file",
    )?;
    logging::log_event(
        config,
        &logging::StoryEvent::simple(
            "QA",
            &story.frontmatter.story_id,
            "QA_CONSENSUS_APPLIED",
            "QA consensus applied (PASS → COMPLETED or FAIL → REFIX)",
        ),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve a path to a project-root-relative string (for @file references).
fn rel_path(config: &Config, path: &Path) -> String {
    path.strip_prefix(&config.project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

/// Extract the numeric NNNN from a story path like `.../story-0007-slug.md`.
/// Returns None when no numeric segment is found.
fn story_num(path: &Path) -> Option<usize> {
    let stem = path.file_stem()?.to_str()?;
    stem.split('-')
        .find(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        .and_then(|n| n.parse().ok())
}

/// Next story number = max existing NNNN + 1 (falls back to 1 when none/empty).
/// Gap-safe: with a deleted/renamed story, `len()+1` would collide with an
/// existing `story-NNNN`; this mirrors the SM prompt's max-based guidance.
fn next_story_num(existing: &[PathBuf]) -> usize {
    existing
        .iter()
        .filter_map(|s| story_num(s))
        .max()
        .unwrap_or(0)
        + 1
}

/// Shared apply-pass runner: invokes the default `pi` command with the given
/// role/model/skill, attaching the consensus file and context files.
fn run_apply_pass(
    config: &Config,
    role_key: &str,
    phase: Phase,
    consensus_rel: &str,
    context_files: &[&str],
    prompt: &str,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_files: Vec<&str> = context_files.to_vec();
    all_files.push(consensus_rel);

    logging::info(&format!("Running pi to {}...", label));
    // The apply pass always uses the default pi command (moa-rust can't write
    // structured frontmatter decisions — it only synthesizes prose consensus).
    let model = config.resolve_model(phase, None);
    let result = invoke_agent(
        config,
        role_key,
        &model,
        &all_files,
        &["--system-prompt", prompt],
    )?;

    if result.success {
        logging::ok(&format!("{} complete.", label));
        Ok(())
    } else {
        // Surface apply-pass failure immediately instead of swallowing it.
        // Callers catch this and treat it like "no consensus applied" so the
        // per-story ambiguity/circuit-breaker machinery sees it on THIS
        // iteration rather than two expensive iterations later.
        Err(format!("{} reported failure: {:?}", label, result.error).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmadder_core::config::Config;

    fn cfg_with_root(root: &std::path::Path) -> Config {
        let toml_path = root.join("bmadder.toml");
        std::fs::write(&toml_path, "").unwrap();
        Config::load(&toml_path).unwrap()
    }

    #[test]
    fn find_latest_moa_output_gates_on_invocation_time() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output");
        std::fs::create_dir_all(&output).unwrap();
        let config = cfg_with_root(dir.path());

        // Pre-existing consensus (stale relative to a future invocation).
        std::fs::write(output.join("moa-stale.md"), "old").unwrap();

        // An invocation starts now; no new file yet → None (stale file skipped).
        let invoked_at = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(15));
        assert_eq!(
            find_latest_moa_output(&config, invoked_at).map(|p| p.file_name().unwrap().to_owned()),
            None,
            "stale consensus must not be picked up"
        );

        // moa-rust writes a fresh consensus after the invocation started.
        std::fs::write(output.join("moa-fresh.md"), "new").unwrap();
        let found = find_latest_moa_output(&config, invoked_at).expect("fresh consensus");
        assert_eq!(found.file_name().unwrap(), "moa-fresh.md");
    }

    #[test]
    fn find_latest_moa_output_none_when_dir_missing() {
        let dir = tempfile::tempdir().unwrap();
        let config = cfg_with_root(dir.path());
        assert_eq!(
            find_latest_moa_output(&config, std::time::SystemTime::now()),
            None
        );
    }

    #[test]
    fn next_story_num_is_max_plus_one_not_len_plus_one() {
        let mk = |n: &str| std::path::PathBuf::from(format!("story-{}-slug.md", n));
        // Gap: 0001 and 0007 exist → next must be 8, not len()+1 == 2.
        let existing = vec![mk("0001"), mk("0007")];
        assert_eq!(next_story_num(&existing), 8);

        // Empty dir → 1.
        assert_eq!(next_story_num(&[]), 1);

        // No numeric segment → falls back to 1.
        let weird = vec![std::path::PathBuf::from("story-notes.md")];
        assert_eq!(next_story_num(&weird), 1);
    }
}
