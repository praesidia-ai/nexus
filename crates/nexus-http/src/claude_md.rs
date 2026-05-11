//! CLAUDE.md as a first-class artifact (NEXUS_MASTER_PLAN §3).
//!
//! Every Nexus-generated project gets a `CLAUDE.md` at its root. It
//! is:
//!
//! - Read on every oneshot / swarm run and prepended to the system
//!   prompt so Claude inherits the project's memory
//! - Written (and merged) whenever the swarm identifies durable
//!   project knowledge (stack decisions, conventions, API shapes…)
//! - Human-editable — it's a plain Markdown file, commit it, diff it,
//!   cherry-pick it
//! - Structured — section headers (`## Architecture`, `## Decisions`,
//!   `## Memory`, `## Conventions`) let us merge new memory slices
//!   into the right place rather than appending to the bottom forever
//!
//! This module does NOT touch the filesystem outside the project
//! root — all operations go through `mini_agents::canonical_child` so
//! sandbox invariants are shared with the mini-agent fleet.

use std::path::{Path, PathBuf};

use crate::mini_agents::canonical_child;

/// The standard filename at the root of every project.
pub const CLAUDE_MD_FILENAME: &str = "CLAUDE.md";

/// Section names we treat as first-class. New sections are allowed —
/// unknown headers round-trip unchanged. The canonical list exists so
/// `promote_memory_slice` knows where to file new knowledge by
/// default.
pub const CANONICAL_SECTIONS: &[&str] = &[
    "Architecture",
    "Decisions",
    "Conventions",
    "Memory",
    "Non-goals",
];

/// A parsed CLAUDE.md — a title, a lead paragraph, and a map of
/// sections keyed by header name. Round-trips losslessly for the
/// sections we touch.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClaudeMd {
    pub title: String,
    pub lead: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Header level (`##` = 2, `###` = 3). Top-level sections default
    /// to 2.
    pub level: u8,
    pub name: String,
    pub body: String,
}

impl ClaudeMd {
    /// Build the bootstrap template used when a project has no
    /// CLAUDE.md yet.
    pub fn bootstrap(project_name: &str) -> Self {
        let sections = CANONICAL_SECTIONS
            .iter()
            .map(|n| Section {
                level: 2,
                name: (*n).to_string(),
                body: String::new(),
            })
            .collect();
        Self {
            title: format!("{project_name} — CLAUDE.md"),
            lead: "Durable project context. Read by every Nexus run; edit freely.".to_string(),
            sections,
        }
    }

    /// Parse a CLAUDE.md from disk text. Unknown structure is
    /// tolerated: content before the first section header is the
    /// `lead`, section bodies carry raw text (blank lines intact).
    pub fn parse(raw: &str) -> Self {
        let mut md = ClaudeMd::default();
        let mut cur: Option<Section> = None;
        let mut in_lead = true;
        let mut lead = String::new();
        for line in raw.lines() {
            if let Some(t) = line.strip_prefix("# ") {
                if md.title.is_empty() {
                    md.title = t.trim().to_string();
                    continue;
                }
            }
            let header = if let Some(n) = line.strip_prefix("## ") {
                Some((2u8, n.trim().to_string()))
            } else {
                line.strip_prefix("### ").map(|n| (3u8, n.trim().to_string()))
            };
            if let Some((level, name)) = header {
                if let Some(s) = cur.take() {
                    md.sections.push(s);
                }
                cur = Some(Section {
                    level,
                    name,
                    body: String::new(),
                });
                in_lead = false;
                continue;
            }
            if in_lead {
                lead.push_str(line);
                lead.push('\n');
            } else if let Some(s) = cur.as_mut() {
                s.body.push_str(line);
                s.body.push('\n');
            }
        }
        if let Some(s) = cur {
            md.sections.push(s);
        }
        md.lead = lead.trim().to_string();
        // Canonicalise section bodies: strip the blank line that
        // conventionally follows a header + the trailing blank line
        // so parse(render(x)) == parse(x).
        for s in md.sections.iter_mut() {
            let trimmed = s.body.trim_start_matches('\n').trim_end_matches('\n');
            s.body = trimmed.to_string();
        }
        md
    }

    /// Render back to disk text.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if !self.title.is_empty() {
            out.push_str("# ");
            out.push_str(&self.title);
            out.push('\n');
            out.push('\n');
        }
        if !self.lead.is_empty() {
            out.push_str(&self.lead);
            out.push_str("\n\n");
        }
        for s in &self.sections {
            for _ in 0..s.level {
                out.push('#');
            }
            out.push(' ');
            out.push_str(&s.name);
            out.push_str("\n\n");
            let body = s.body.trim_end_matches('\n');
            out.push_str(body);
            if !body.is_empty() {
                out.push('\n');
            }
            out.push('\n');
        }
        out
    }

    /// Append a bullet to a named section (create it if missing).
    /// Duplicate bullets are de-duplicated verbatim so repeated swarm
    /// runs don't accumulate noise.
    pub fn promote_memory_slice(&mut self, section_name: &str, bullet: &str) -> bool {
        let bullet = bullet.trim();
        if bullet.is_empty() {
            return false;
        }
        let line = format!("- {bullet}");
        let idx = self
            .sections
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(section_name));
        let idx = match idx {
            Some(i) => i,
            None => {
                self.sections.push(Section {
                    level: 2,
                    name: section_name.to_string(),
                    body: String::new(),
                });
                self.sections.len() - 1
            }
        };
        let body = &mut self.sections[idx].body;
        if body.lines().any(|l| l.trim() == line) {
            return false;
        }
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str(&line);
        true
    }
}

/// Read the CLAUDE.md for a project. Returns a bootstrap document
/// when the file is missing so the caller always has something to
/// pass to the system prompt.
pub async fn load(project_root: &Path, project_name: &str) -> std::io::Result<ClaudeMd> {
    let path = claude_md_path(project_root)?;
    match tokio::fs::read_to_string(&path).await {
        Ok(raw) => Ok(ClaudeMd::parse(&raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(ClaudeMd::bootstrap(project_name))
        }
        Err(e) => Err(e),
    }
}

/// Atomic tmp→fsync→rename write of the rendered CLAUDE.md.
pub async fn save(project_root: &Path, md: &ClaudeMd) -> std::io::Result<()> {
    let path = claude_md_path(project_root)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let body = md.render();
    let mut tmp = path.clone();
    let mut fname = path
        .file_name()
        .unwrap_or_default()
        .to_os_string();
    fname.push(".nexus-tmp");
    tmp.set_file_name(fname);
    tokio::fs::write(&tmp, body.as_bytes()).await?;
    if let Ok(f) = tokio::fs::File::open(&tmp).await {
        let _ = f.sync_all().await;
    }
    tokio::fs::rename(&tmp, &path).await?;
    Ok(())
}

/// Render the CLAUDE.md as a single system-prompt block. Caller
/// prepends this to the user's own system prompt so Claude always
/// sees project memory first.
pub fn as_system_prompt(md: &ClaudeMd) -> String {
    // Keep the marker stable so downstream tools can grep / strip it.
    format!(
        "<nexus:claude_md>\n{}\n</nexus:claude_md>",
        md.render().trim_end_matches('\n')
    )
}

fn claude_md_path(project_root: &Path) -> std::io::Result<PathBuf> {
    // Make sure the root exists so canonical_child can resolve it.
    std::fs::create_dir_all(project_root)?;
    canonical_child(project_root, CLAUDE_MD_FILENAME)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_has_canonical_sections() {
        let md = ClaudeMd::bootstrap("Demo");
        assert!(md.title.contains("Demo"));
        for name in CANONICAL_SECTIONS {
            assert!(md.sections.iter().any(|s| s.name == *name), "missing {name}");
        }
    }

    #[test]
    fn parse_then_render_roundtrips_known_structure() {
        let src = "# My Project\n\nlead text\n\n## Architecture\n\n- axum backend\n- next.js frontend\n\n## Decisions\n\n- SQLite default\n";
        let md = ClaudeMd::parse(src);
        assert_eq!(md.title, "My Project");
        assert_eq!(md.lead, "lead text");
        let out = md.render();
        // Reparsing should yield the same structure.
        let md2 = ClaudeMd::parse(&out);
        assert_eq!(md, md2);
    }

    #[test]
    fn promote_memory_slice_appends_new_bullet() {
        let mut md = ClaudeMd::bootstrap("Demo");
        assert!(md.promote_memory_slice("Decisions", "use qwen3-coder:7b for leaves"));
        let decisions = md
            .sections
            .iter()
            .find(|s| s.name == "Decisions")
            .unwrap();
        assert!(decisions.body.contains("use qwen3-coder:7b for leaves"));
    }

    #[test]
    fn promote_memory_slice_dedupes() {
        let mut md = ClaudeMd::bootstrap("Demo");
        assert!(md.promote_memory_slice("Memory", "user prefers Claude Sonnet"));
        assert!(!md.promote_memory_slice("Memory", "user prefers Claude Sonnet"));
        let memory = md.sections.iter().find(|s| s.name == "Memory").unwrap();
        let count = memory
            .body
            .lines()
            .filter(|l| l.contains("user prefers Claude Sonnet"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn promote_memory_slice_creates_missing_section() {
        let mut md = ClaudeMd::bootstrap("Demo");
        assert!(md.promote_memory_slice("Playbook", "step 1"));
        assert!(md.sections.iter().any(|s| s.name == "Playbook"));
    }

    #[tokio::test]
    async fn load_returns_bootstrap_when_missing() {
        let dir = tempdir().unwrap();
        let md = load(dir.path(), "Demo").await.unwrap();
        assert_eq!(md.title, "Demo — CLAUDE.md");
    }

    #[tokio::test]
    async fn save_then_load_roundtrip() {
        let dir = tempdir().unwrap();
        let mut md = ClaudeMd::bootstrap("Demo");
        md.promote_memory_slice("Memory", "Nova writes app/page.tsx");
        save(dir.path(), &md).await.unwrap();
        let loaded = load(dir.path(), "Demo").await.unwrap();
        assert_eq!(loaded.title, md.title);
        let memory = loaded
            .sections
            .iter()
            .find(|s| s.name == "Memory")
            .unwrap();
        assert!(memory.body.contains("Nova writes app/page.tsx"));
    }

    #[test]
    fn system_prompt_wraps_in_marker() {
        let md = ClaudeMd::bootstrap("Demo");
        let sp = as_system_prompt(&md);
        assert!(sp.starts_with("<nexus:claude_md>"));
        assert!(sp.ends_with("</nexus:claude_md>"));
    }
}
