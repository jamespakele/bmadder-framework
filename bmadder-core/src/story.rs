use serde::{Deserialize, Serialize};
use std::fmt;

/// Story pipeline status — matches the BMADder state machine exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StoryStatus {
    Draft,
    Revise,
    ReadyForDev,
    InDev,
    #[serde(rename = "PENDING_QA")]
    PendingQA,
    Refix,
    Completed,
}

impl StoryStatus {
    /// All statuses in display order (matches bash `show_status`).
    pub fn all() -> [StoryStatus; 7] {
        [
            StoryStatus::Draft,
            StoryStatus::Revise,
            StoryStatus::ReadyForDev,
            StoryStatus::InDev,
            StoryStatus::PendingQA,
            StoryStatus::Refix,
            StoryStatus::Completed,
        ]
    }

    /// Human-readable label matching bash output.
    pub fn label(&self) -> &'static str {
        match self {
            StoryStatus::Draft => "DRAFT",
            StoryStatus::Revise => "REVISE",
            StoryStatus::ReadyForDev => "READY_FOR_DEV",
            StoryStatus::InDev => "IN_DEV",
            StoryStatus::PendingQA => "PENDING_QA",
            StoryStatus::Refix => "REFIX",
            StoryStatus::Completed => "COMPLETED",
        }
    }
}

impl fmt::Display for StoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// YAML frontmatter parsed from a story file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryFrontmatter {
    #[serde(default)]
    pub story_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic_id: Option<String>,
    pub title: String,
    pub status: StoryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_dev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub po_alignment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qa_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
}

/// A full story — frontmatter + markdown body.
#[derive(Debug, Clone)]
pub struct Story {
    pub path: std::path::PathBuf,
    pub frontmatter: StoryFrontmatter,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_display_matches_bash() {
        assert_eq!(StoryStatus::Draft.to_string(), "DRAFT");
        assert_eq!(StoryStatus::Revise.to_string(), "REVISE");
        assert_eq!(StoryStatus::ReadyForDev.to_string(), "READY_FOR_DEV");
        assert_eq!(StoryStatus::InDev.to_string(), "IN_DEV");
        assert_eq!(StoryStatus::PendingQA.to_string(), "PENDING_QA");
        assert_eq!(StoryStatus::Refix.to_string(), "REFIX");
        assert_eq!(StoryStatus::Completed.to_string(), "COMPLETED");
    }

    #[test]
    fn status_serialize_roundtrip() {
        for status in StoryStatus::all() {
            let json = serde_json::to_string(&status).unwrap();
            let back: StoryStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn status_yaml_roundtrip() {
        for status in StoryStatus::all() {
            let yaml = serde_yaml::to_string(&status).unwrap();
            let back: StoryStatus = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn frontmatter_serde_roundtrip() {
        let fm = StoryFrontmatter {
            story_id: "STORY-0001".into(),
            epic_id: Some("EPIC-0001".into()),
            title: "Test Story".into(),
            status: StoryStatus::Draft,
            priority: Some("P0".into()),
            agent_hint: Some("codex".into()),
            assigned_dev: None,
            po_alignment: Some("PENDING".into()),
            qa_status: None,
            created_at: Some("2026-06-18".into()),
            updated_at: Some("2026-06-18".into()),
            links: vec!["docs/arch.md#auth".into()],
        };
        let yaml = serde_yaml::to_string(&fm).unwrap();
        let back: StoryFrontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(fm.story_id, back.story_id);
        assert_eq!(fm.title, back.title);
        assert_eq!(fm.status, back.status);
        assert_eq!(fm.agent_hint, back.agent_hint);
        assert_eq!(fm.links, back.links);
    }

    #[test]
    fn frontmatter_minimal_fields() {
        let yaml = r#"story_id: "STORY-0002"
title: "Minimal"
status: DRAFT
"#;
        let fm: StoryFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.story_id, "STORY-0002");
        assert!(fm.epic_id.is_none());
        assert!(fm.agent_hint.is_none());
        assert!(fm.links.is_empty());
    }
}
