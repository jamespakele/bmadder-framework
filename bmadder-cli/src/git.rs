use std::path::Path;
use std::process::Command;

/// Run `git add -A` in the project root.
pub fn git_add_all(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Run `git commit -m "..."` with --allow-empty.
pub fn git_commit(project_root: &Path, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["commit", "--allow-empty", "-m", message])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Run `git push`. Best-effort: logs warning on failure, never halts pipeline.
pub fn git_push(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["push"])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        crate::logging::warn(&format!(
            "git push failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// Pre-dev snapshot: `git add -A && git commit -m "chore: pre-dev worktree snapshot"`.
pub fn git_snapshot(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    git_add_all(project_root)?;
    // Allow empty — there may be nothing to commit
    let _ = git_commit(project_root, "chore: pre-dev worktree snapshot");
    Ok(())
}

/// QA-pass commit: `git add -A && git commit -m "story($story_id): $title [QA PASS]"`.
pub fn git_story_commit(
    project_root: &Path,
    story_id: &str,
    title: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    git_add_all(project_root)?;
    let msg = format!("story({}): {} [QA PASS]", story_id, title);
    git_commit(project_root, &msg)?;
    git_push(project_root)?;
    Ok(())
}

/// Discard uncommitted changes: `git checkout .`.
#[allow(dead_code)]
pub fn git_clean_worktree(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["checkout", "."])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git checkout failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Check if the working tree is clean (no uncommitted changes, no untracked files).
#[allow(dead_code)]
pub fn git_is_clean(project_root: &Path) -> bool {
    // Check for uncommitted changes
    let diff = Command::new("git")
        .args(["diff", "--quiet", "HEAD"])
        .current_dir(project_root)
        .status();
    let has_changes = diff.map(|s| !s.success()).unwrap_or(true);

    // Check for untracked files
    let untracked = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(project_root)
        .output();
    let has_untracked = untracked.map(|o| !o.stdout.is_empty()).unwrap_or(true);

    !has_changes && !has_untracked
}

/// Initialize a git repo if none exists. Returns true if initialized.
pub fn git_init_if_needed(project_root: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    if project_root.join(".git").exists() {
        return Ok(false);
    }
    Command::new("git")
        .args(["init"])
        .current_dir(project_root)
        .output()?;
    git_add_all(project_root)?;
    git_commit(project_root, "chore: initialize BMADder project")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_init_and_commit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Init
        let initialized = git_init_if_needed(root).unwrap();
        assert!(initialized);

        // Should not init again
        let initialized2 = git_init_if_needed(root).unwrap();
        assert!(!initialized2);

        // Create a file and commit
        std::fs::write(root.join("test.txt"), "hello").unwrap();
        git_add_all(root).unwrap();
        git_commit(root, "test commit").unwrap();

        // Clean check
        assert!(git_is_clean(root));
    }

    #[test]
    fn test_git_snapshot_and_story_commit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_init_if_needed(root).unwrap();

        // Snapshot on clean tree
        git_snapshot(root).unwrap();

        // Create a file, snapshot
        std::fs::write(root.join("code.rs"), "fn main() {}").unwrap();
        git_snapshot(root).unwrap();
        assert!(git_is_clean(root));

        // Story commit
        std::fs::write(root.join("code.rs"), "fn main() { println!(\"hi\"); }").unwrap();
        // Story commit will try to push — that's fine, it'll warn but not fail
        let _ = git_story_commit(root, "STORY-0001", "Test Story");
        assert!(git_is_clean(root));
    }

    #[test]
    fn test_git_clean_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_init_if_needed(root).unwrap();

        std::fs::write(root.join("dirty.txt"), "dirty").unwrap();
        git_add_all(root).unwrap();
        git_commit(root, "base").unwrap();

        // Modify the file
        std::fs::write(root.join("dirty.txt"), "modified").unwrap();
        assert!(!git_is_clean(root));

        // Clean it
        git_clean_worktree(root).unwrap();
        assert!(git_is_clean(root));
        assert_eq!(
            std::fs::read_to_string(root.join("dirty.txt")).unwrap(),
            "dirty"
        );
    }
}
