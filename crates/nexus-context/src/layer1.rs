use serde::{Deserialize, Serialize};

/// L1 — Project Index: always loaded, provides high-level project awareness.
/// This layer is extremely compact (~2K tokens) and never evicted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub project_id: String,
    pub project_name: String,
    pub description: String,
    pub tech_stack: Vec<String>,
    pub key_directories: Vec<DirectoryEntry>,
    pub active_goals: Vec<String>,
    pub key_decisions: Vec<Decision>,
    pub team_members: Vec<String>,
    pub constraints: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub path: String,
    pub purpose: String,
    pub file_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub summary: String,
    pub rationale: String,
    pub decided_at: String,
}

impl ProjectIndex {
    pub fn new(project_id: &str, name: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
            project_name: name.to_string(),
            description: String::new(),
            tech_stack: Vec::new(),
            key_directories: Vec::new(),
            active_goals: Vec::new(),
            key_decisions: Vec::new(),
            team_members: Vec::new(),
            constraints: Vec::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Estimate token count for this index (rough: 4 chars per token).
    pub fn estimated_tokens(&self) -> usize {
        let json = serde_json::to_string(self).unwrap_or_default();
        json.len() / 4
    }

    /// Render as a compact context block for injection into prompts.
    pub fn render(&self) -> String {
        let mut out = format!("# Project: {}\n", self.project_name);
        if !self.description.is_empty() {
            out.push_str(&format!("{}\n", self.description));
        }
        if !self.tech_stack.is_empty() {
            out.push_str(&format!("Tech: {}\n", self.tech_stack.join(", ")));
        }
        for dir in &self.key_directories {
            out.push_str(&format!("- {}: {} ({} files)\n", dir.path, dir.purpose, dir.file_count));
        }
        for goal in &self.active_goals {
            out.push_str(&format!("- Goal: {goal}\n"));
        }
        for dec in &self.key_decisions {
            out.push_str(&format!("- Decision: {} ({})\n", dec.summary, dec.rationale));
        }
        if !self.constraints.is_empty() {
            out.push_str(&format!("Constraints: {}\n", self.constraints.join(", ")));
        }
        out
    }

    pub fn add_decision(&mut self, summary: &str, rationale: &str) {
        self.key_decisions.push(Decision {
            id: uuid::Uuid::new_v4().to_string(),
            summary: summary.to_string(),
            rationale: rationale.to_string(),
            decided_at: chrono::Utc::now().to_rfc3339(),
        });
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_index_has_defaults() {
        let idx = ProjectIndex::new("proj-1", "Test Project");
        assert_eq!(idx.project_id, "proj-1");
        assert_eq!(idx.project_name, "Test Project");
        assert!(idx.tech_stack.is_empty());
        assert!(idx.key_decisions.is_empty());
    }

    #[test]
    fn render_includes_name_and_tech() {
        let mut idx = ProjectIndex::new("p1", "Nexus");
        idx.tech_stack = vec!["Rust".into(), "TypeScript".into()];
        idx.description = "AI platform".into();
        let rendered = idx.render();
        assert!(rendered.contains("# Project: Nexus"));
        assert!(rendered.contains("AI platform"));
        assert!(rendered.contains("Tech: Rust, TypeScript"));
    }

    #[test]
    fn render_includes_goals_and_decisions() {
        let mut idx = ProjectIndex::new("p1", "Nexus");
        idx.active_goals = vec!["Ship v1".into()];
        idx.add_decision("Use Rust", "Performance");
        let rendered = idx.render();
        assert!(rendered.contains("- Goal: Ship v1"));
        assert!(rendered.contains("- Decision: Use Rust (Performance)"));
    }

    #[test]
    fn render_includes_directories() {
        let mut idx = ProjectIndex::new("p1", "Nexus");
        idx.key_directories.push(DirectoryEntry {
            path: "src/".into(),
            purpose: "Source code".into(),
            file_count: 42,
        });
        let rendered = idx.render();
        assert!(rendered.contains("- src/: Source code (42 files)"));
    }

    #[test]
    fn estimated_tokens_is_proportional_to_content() {
        let small = ProjectIndex::new("p1", "Small");
        let mut big = ProjectIndex::new("p2", "Big");
        big.description = "A".repeat(1000);
        assert!(big.estimated_tokens() > small.estimated_tokens());
    }

    #[test]
    fn add_decision_updates_timestamp() {
        let mut idx = ProjectIndex::new("p1", "Test");
        let before = idx.updated_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(10));
        idx.add_decision("Choose SQLite", "Simplicity");
        assert_ne!(idx.updated_at, before);
        assert_eq!(idx.key_decisions.len(), 1);
        assert_eq!(idx.key_decisions[0].summary, "Choose SQLite");
    }
}
