//! Community marketplace — contribution registry for agents, teams, plugins, domain packs.
//!
//! Provides a local registry for installing, managing, and searching community contributions.
//! Contributions follow a standard manifest format and are stored on disk at
//! `<data_dir>/community/registry.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Manifest types
// ---------------------------------------------------------------------------

/// Standard manifest for all community contributions.
///
/// Every contribution — whether an agent definition, team template, plugin, domain pack,
/// or tool extension — is described by a single `ContributionManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionManifest {
    /// Globally unique contribution identifier (e.g. `"acme/code-reviewer"`).
    pub id: String,
    /// Human-readable name shown in the marketplace UI.
    pub name: String,
    /// Short description (one or two sentences).
    pub description: String,
    /// SemVer version string (e.g. `"1.2.0"`).
    pub version: String,
    /// Author information.
    pub author: ContributionAuthor,
    /// Top-level category for search and filtering.
    pub category: ContributionCategory,
    /// SPDX licence identifier (e.g. `"MIT"`, `"Apache-2.0"`).
    pub license: String,
    /// Source repository URL (optional).
    pub repository: Option<String>,
    /// Free-form tags for discovery.
    pub tags: Vec<String>,
    /// SemVer range of Nexus versions this contribution is compatible with.
    pub nexus_version_range: String,
    /// Long-form readme content (Markdown).
    pub readme: String,
    /// The actual contribution payload — type-tagged for safe deserialization.
    pub payload: ContributionPayload,
}

/// Author metadata for a contribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionAuthor {
    /// Author display name.
    pub name: String,
    /// Contact email (optional).
    pub email: Option<String>,
    /// Author homepage or profile URL (optional).
    pub url: Option<String>,
}

/// Top-level category that every contribution belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContributionCategory {
    /// A complete agent definition (role, model, skills).
    AgentDefinition,
    /// A reusable team template (agent roster + topology).
    TeamTemplate,
    /// A plugin with lifecycle hooks.
    Plugin,
    /// A domain knowledge pack (prompts, examples, schemas).
    DomainPack,
    /// A single tool extension (MCP tool, CLI wrapper, etc.).
    ToolExtension,
}

impl std::fmt::Display for ContributionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentDefinition => write!(f, "agent_definition"),
            Self::TeamTemplate => write!(f, "team_template"),
            Self::Plugin => write!(f, "plugin"),
            Self::DomainPack => write!(f, "domain_pack"),
            Self::ToolExtension => write!(f, "tool_extension"),
        }
    }
}

/// Type-tagged payload for each contribution category.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContributionPayload {
    /// Full agent definition (JSON blob).
    Agent {
        /// The agent definition data.
        definition: serde_json::Value,
    },
    /// Team template (JSON blob).
    Team {
        /// The team template data.
        template: serde_json::Value,
    },
    /// Plugin manifest (JSON blob).
    Plugin {
        /// The plugin manifest data.
        manifest: serde_json::Value,
    },
    /// Domain knowledge pack (JSON blob).
    DomainPack {
        /// The domain pack data.
        pack: serde_json::Value,
    },
    /// Tool extension definition (JSON blob).
    Tool {
        /// The tool definition data.
        definition: serde_json::Value,
    },
}

// ---------------------------------------------------------------------------
// Installed contribution wrapper
// ---------------------------------------------------------------------------

/// A contribution that has been installed into the local registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledContribution {
    /// The full manifest.
    pub manifest: ContributionManifest,
    /// ISO-8601 timestamp of when the contribution was installed.
    pub installed_at: String,
    /// Whether the contribution is currently enabled.
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Local registry
// ---------------------------------------------------------------------------

/// Local contribution registry — persists installed contributions on disk.
///
/// Data lives at `<registry_dir>/registry.json`. The registry is loaded into
/// memory on startup and flushed after every mutation.
pub struct LocalRegistry {
    /// Directory where `registry.json` is stored.
    registry_dir: PathBuf,
    /// In-memory list of installed contributions.
    contributions: Vec<InstalledContribution>,
}

impl LocalRegistry {
    /// Create a new `LocalRegistry` rooted at `data_dir/community/`.
    pub fn new(data_dir: &std::path::Path) -> Self {
        let registry_dir = data_dir.join("community");
        Self {
            registry_dir,
            contributions: Vec::new(),
        }
    }

    /// Load the registry from disk. Creates the directory and an empty registry
    /// file if they do not exist.
    pub fn load(&mut self) -> Result<(), String> {
        std::fs::create_dir_all(&self.registry_dir)
            .map_err(|e| format!("failed to create registry dir: {e}"))?;

        let path = self.registry_dir.join("registry.json");
        if path.exists() {
            let data = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read registry: {e}"))?;
            self.contributions = serde_json::from_str(&data)
                .map_err(|e| format!("failed to parse registry: {e}"))?;
        } else {
            self.contributions = Vec::new();
            self.flush()?;
        }
        Ok(())
    }

    /// Install a contribution. Replaces any existing contribution with the same ID.
    pub fn install(&mut self, manifest: ContributionManifest) -> Result<(), String> {
        let issues = validate_manifest(&manifest);
        let has_errors = issues.iter().any(|i| i.severity == ValidationSeverity::Error);
        if has_errors {
            let msgs: Vec<String> = issues
                .iter()
                .filter(|i| i.severity == ValidationSeverity::Error)
                .map(|i| format!("{}: {}", i.field, i.message))
                .collect();
            return Err(format!("manifest validation failed: {}", msgs.join("; ")));
        }

        // Remove existing version if present (upgrade path).
        self.contributions.retain(|c| c.manifest.id != manifest.id);

        let now = chrono::Utc::now().to_rfc3339();
        self.contributions.push(InstalledContribution {
            manifest,
            installed_at: now,
            enabled: true,
        });
        self.flush()
    }

    /// Uninstall a contribution by ID.
    pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
        let before = self.contributions.len();
        self.contributions.retain(|c| c.manifest.id != id);
        if self.contributions.len() == before {
            return Err(format!("contribution '{id}' not found"));
        }
        self.flush()
    }

    /// Return a slice of all installed contributions.
    pub fn list(&self) -> &[InstalledContribution] {
        &self.contributions
    }

    /// Look up a single installed contribution by ID.
    pub fn get(&self, id: &str) -> Option<&InstalledContribution> {
        self.contributions.iter().find(|c| c.manifest.id == id)
    }

    /// Full-text search across name, description, and tags.
    pub fn search(&self, query: &str) -> Vec<&InstalledContribution> {
        let q = query.to_lowercase();
        self.contributions
            .iter()
            .filter(|c| {
                c.manifest.name.to_lowercase().contains(&q)
                    || c.manifest.description.to_lowercase().contains(&q)
                    || c.manifest.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Filter installed contributions by category.
    pub fn by_category(&self, category: &ContributionCategory) -> Vec<&InstalledContribution> {
        self.contributions
            .iter()
            .filter(|c| &c.manifest.category == category)
            .collect()
    }

    // -- private helpers -----------------------------------------------------

    /// Flush the in-memory registry to disk.
    fn flush(&self) -> Result<(), String> {
        let path = self.registry_dir.join("registry.json");
        let data = serde_json::to_string_pretty(&self.contributions)
            .map_err(|e| format!("failed to serialize registry: {e}"))?;
        std::fs::write(&path, data)
            .map_err(|e| format!("failed to write registry: {e}"))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A single issue found during manifest validation.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    /// How severe the issue is.
    pub severity: ValidationSeverity,
    /// The manifest field that has the issue.
    pub field: String,
    /// Human-readable description of the issue.
    pub message: String,
}

/// Severity level for a [`ValidationIssue`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValidationSeverity {
    /// The manifest is invalid and cannot be installed.
    Error,
    /// The manifest is valid but has a potential problem.
    Warning,
    /// Informational suggestion for improvement.
    Info,
}

/// Validate a contribution manifest and return all issues found.
///
/// Returns an empty vec if the manifest is valid.
pub fn validate_manifest(manifest: &ContributionManifest) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Required non-empty string fields.
    if manifest.id.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "id".into(),
            message: "id must not be empty".into(),
        });
    }
    if manifest.name.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "name".into(),
            message: "name must not be empty".into(),
        });
    }
    if manifest.description.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "description".into(),
            message: "description must not be empty".into(),
        });
    }
    if manifest.version.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "version".into(),
            message: "version must not be empty".into(),
        });
    }
    if manifest.author.name.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "author.name".into(),
            message: "author name must not be empty".into(),
        });
    }
    if manifest.license.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "license".into(),
            message: "license must not be empty".into(),
        });
    }
    if manifest.nexus_version_range.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "nexus_version_range".into(),
            message: "nexus_version_range must not be empty".into(),
        });
    }

    // Version format check (loose SemVer: major.minor.patch).
    let parts: Vec<&str> = manifest.version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.parse::<u64>().is_err()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            field: "version".into(),
            message: "version must be valid semver (e.g. '1.0.0')".into(),
        });
    }

    // Warnings.
    if manifest.tags.is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            field: "tags".into(),
            message: "adding tags improves discoverability".into(),
        });
    }
    if manifest.readme.trim().is_empty() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            field: "readme".into(),
            message: "a readme helps users understand the contribution".into(),
        });
    }
    if manifest.repository.is_none() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Info,
            field: "repository".into(),
            message: "consider linking to the source repository".into(),
        });
    }

    issues
}

// ---------------------------------------------------------------------------
// Compatibility checking
// ---------------------------------------------------------------------------

/// Result of checking version compatibility between a contribution and a Nexus version.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CompatibilityResult {
    /// The contribution is fully compatible.
    Compatible,
    /// The contribution may work but some features could be missing.
    Degraded(String),
    /// The contribution will not work with this Nexus version.
    Incompatible(String),
}

/// Check whether a contribution's `version_range` is compatible with the running
/// `nexus_version`.
///
/// Supports simple range formats:
/// - `">=1.0.0"` — minimum version
/// - `">=1.0.0 <2.0.0"` — bounded range
/// - `"*"` — any version
///
/// Falls back to `Degraded` if the range cannot be parsed.
pub fn check_compatibility(version_range: &str, nexus_version: &str) -> CompatibilityResult {
    let range = version_range.trim();

    // Wildcard — always compatible.
    if range == "*" {
        return CompatibilityResult::Compatible;
    }

    let nv = match parse_semver(nexus_version) {
        Some(v) => v,
        None => {
            return CompatibilityResult::Degraded(
                format!("cannot parse nexus version '{nexus_version}'"),
            );
        }
    };

    // Try to parse simple range expressions.
    let parts: Vec<&str> = range.split_whitespace().collect();
    let mut lower: Option<(u64, u64, u64)> = None;
    let mut upper: Option<(u64, u64, u64)> = None;

    for part in &parts {
        if let Some(rest) = part.strip_prefix(">=") {
            lower = parse_semver(rest);
        } else if let Some(rest) = part.strip_prefix('<') {
            upper = parse_semver(rest);
        } else if parse_semver(part).is_some() {
            // Exact version — treat as lower bound.
            lower = parse_semver(part);
        }
    }

    if lower.is_none() && upper.is_none() {
        return CompatibilityResult::Degraded(format!(
            "cannot parse version range '{range}'; assuming compatible"
        ));
    }

    if let Some(lo) = lower {
        if nv < lo {
            return CompatibilityResult::Incompatible(format!(
                "requires nexus >= {}.{}.{}, but running {nexus_version}",
                lo.0, lo.1, lo.2,
            ));
        }
    }

    if let Some(hi) = upper {
        if nv >= hi {
            return CompatibilityResult::Incompatible(format!(
                "requires nexus < {}.{}.{}, but running {nexus_version}",
                hi.0, hi.1, hi.2,
            ));
        }
    }

    CompatibilityResult::Compatible
}

/// Parse a simple `"major.minor.patch"` string into a tuple.
fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].parse().ok()?;
    Some((major, minor, patch))
}

// ---------------------------------------------------------------------------
// Featured contributions (static seed for MVP)
// ---------------------------------------------------------------------------

/// Return a curated list of featured contributions for the marketplace landing page.
pub fn featured_contributions() -> Vec<ContributionManifest> {
    vec![
        ContributionManifest {
            id: "nexus/code-reviewer-agent".into(),
            name: "Code Reviewer Agent".into(),
            description: "An agent that reviews PRs for style, bugs, and performance issues.".into(),
            version: "1.0.0".into(),
            author: ContributionAuthor {
                name: "Nexus Team".into(),
                email: Some("team@nexus.dev".into()),
                url: None,
            },
            category: ContributionCategory::AgentDefinition,
            license: "MIT".into(),
            repository: Some("https://github.com/nexus/code-reviewer-agent".into()),
            tags: vec!["code-review".into(), "quality".into(), "ci".into()],
            nexus_version_range: ">=0.1.0".into(),
            readme: "# Code Reviewer Agent\n\nAutomatically reviews code for common issues.".into(),
            payload: ContributionPayload::Agent {
                definition: serde_json::json!({
                    "role": "code-reviewer",
                    "model": "gpt-4o",
                    "skills": ["lint", "security-scan", "style-check"]
                }),
            },
        },
        ContributionManifest {
            id: "nexus/fullstack-team".into(),
            name: "Full-Stack Team".into(),
            description: "A pre-configured team of frontend, backend, and devops agents.".into(),
            version: "1.0.0".into(),
            author: ContributionAuthor {
                name: "Nexus Team".into(),
                email: Some("team@nexus.dev".into()),
                url: None,
            },
            category: ContributionCategory::TeamTemplate,
            license: "MIT".into(),
            repository: Some("https://github.com/nexus/fullstack-team".into()),
            tags: vec!["team".into(), "fullstack".into(), "web".into()],
            nexus_version_range: ">=0.1.0".into(),
            readme: "# Full-Stack Team\n\nA balanced team for building web applications.".into(),
            payload: ContributionPayload::Team {
                template: serde_json::json!({
                    "members": ["frontend", "backend", "devops"],
                    "topology": "star"
                }),
            },
        },
        ContributionManifest {
            id: "nexus/sentry-plugin".into(),
            name: "Sentry Error Tracking Plugin".into(),
            description: "Automatically captures and reports errors to Sentry.".into(),
            version: "1.0.0".into(),
            author: ContributionAuthor {
                name: "Nexus Team".into(),
                email: Some("team@nexus.dev".into()),
                url: None,
            },
            category: ContributionCategory::Plugin,
            license: "MIT".into(),
            repository: None,
            tags: vec!["sentry".into(), "errors".into(), "observability".into()],
            nexus_version_range: "*".into(),
            readme: "# Sentry Plugin\n\nIntegrates Sentry error tracking into your Nexus project.".into(),
            payload: ContributionPayload::Plugin {
                manifest: serde_json::json!({
                    "hooks": ["post-build", "on-error"],
                    "config": { "dsn": "" }
                }),
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal valid manifest for testing.
    fn valid_manifest() -> ContributionManifest {
        ContributionManifest {
            id: "test/my-agent".into(),
            name: "My Agent".into(),
            description: "A test agent.".into(),
            version: "1.0.0".into(),
            author: ContributionAuthor {
                name: "Tester".into(),
                email: None,
                url: None,
            },
            category: ContributionCategory::AgentDefinition,
            license: "MIT".into(),
            repository: None,
            tags: vec!["test".into()],
            nexus_version_range: ">=0.1.0".into(),
            readme: "# My Agent".into(),
            payload: ContributionPayload::Agent {
                definition: serde_json::json!({"role": "tester"}),
            },
        }
    }

    // -- Manifest validation -------------------------------------------------

    #[test]
    fn valid_manifest_has_no_errors() {
        let issues = validate_manifest(&valid_manifest());
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
            .collect();
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn missing_id_is_error() {
        let mut m = valid_manifest();
        m.id = String::new();
        let issues = validate_manifest(&m);
        assert!(issues.iter().any(|i| i.field == "id" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn missing_name_is_error() {
        let mut m = valid_manifest();
        m.name = String::new();
        let issues = validate_manifest(&m);
        assert!(issues.iter().any(|i| i.field == "name" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn invalid_version_format() {
        let mut m = valid_manifest();
        m.version = "not-a-version".into();
        let issues = validate_manifest(&m);
        assert!(issues.iter().any(|i| i.field == "version" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn missing_license_is_error() {
        let mut m = valid_manifest();
        m.license = String::new();
        let issues = validate_manifest(&m);
        assert!(issues.iter().any(|i| i.field == "license" && i.severity == ValidationSeverity::Error));
    }

    #[test]
    fn empty_tags_is_warning() {
        let mut m = valid_manifest();
        m.tags = vec![];
        let issues = validate_manifest(&m);
        assert!(issues.iter().any(|i| i.field == "tags" && i.severity == ValidationSeverity::Warning));
    }

    #[test]
    fn no_repository_is_info() {
        let m = valid_manifest(); // repository is already None
        let issues = validate_manifest(&m);
        assert!(issues.iter().any(|i| i.field == "repository" && i.severity == ValidationSeverity::Info));
    }

    // -- Local registry ------------------------------------------------------

    #[test]
    fn registry_install_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();
        assert_eq!(reg.list().len(), 0);

        reg.install(valid_manifest()).unwrap();
        assert_eq!(reg.list().len(), 1);
        assert_eq!(reg.list()[0].manifest.id, "test/my-agent");
        assert!(reg.list()[0].enabled);
    }

    #[test]
    fn registry_install_replaces_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();

        reg.install(valid_manifest()).unwrap();
        let mut updated = valid_manifest();
        updated.version = "2.0.0".into();
        reg.install(updated).unwrap();

        assert_eq!(reg.list().len(), 1);
        assert_eq!(reg.list()[0].manifest.version, "2.0.0");
    }

    #[test]
    fn registry_uninstall() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();
        reg.install(valid_manifest()).unwrap();

        reg.uninstall("test/my-agent").unwrap();
        assert_eq!(reg.list().len(), 0);
    }

    #[test]
    fn registry_uninstall_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();
        let result = reg.uninstall("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn registry_get() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();
        reg.install(valid_manifest()).unwrap();

        assert!(reg.get("test/my-agent").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_search() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();
        reg.install(valid_manifest()).unwrap();

        let results = reg.search("agent");
        assert_eq!(results.len(), 1);

        let results = reg.search("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn registry_by_category() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();
        reg.install(valid_manifest()).unwrap();

        let agents = reg.by_category(&ContributionCategory::AgentDefinition);
        assert_eq!(agents.len(), 1);

        let plugins = reg.by_category(&ContributionCategory::Plugin);
        assert!(plugins.is_empty());
    }

    #[test]
    fn registry_persists_across_load() {
        let tmp = tempfile::tempdir().unwrap();

        // Install, then drop.
        {
            let mut reg = LocalRegistry::new(tmp.path());
            reg.load().unwrap();
            reg.install(valid_manifest()).unwrap();
        }

        // Reload from disk.
        let mut reg2 = LocalRegistry::new(tmp.path());
        reg2.load().unwrap();
        assert_eq!(reg2.list().len(), 1);
        assert_eq!(reg2.list()[0].manifest.id, "test/my-agent");
    }

    #[test]
    fn registry_rejects_invalid_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let mut reg = LocalRegistry::new(tmp.path());
        reg.load().unwrap();

        let mut m = valid_manifest();
        m.id = String::new(); // invalid
        let result = reg.install(m);
        assert!(result.is_err());
        assert_eq!(reg.list().len(), 0);
    }

    // -- Compatibility checking ----------------------------------------------

    #[test]
    fn wildcard_is_always_compatible() {
        match check_compatibility("*", "1.2.3") {
            CompatibilityResult::Compatible => {}
            other => panic!("expected Compatible, got {other:?}"),
        }
    }

    #[test]
    fn gte_range_compatible() {
        match check_compatibility(">=1.0.0", "1.2.3") {
            CompatibilityResult::Compatible => {}
            other => panic!("expected Compatible, got {other:?}"),
        }
    }

    #[test]
    fn gte_range_incompatible() {
        match check_compatibility(">=2.0.0", "1.2.3") {
            CompatibilityResult::Incompatible(_) => {}
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    #[test]
    fn bounded_range_compatible() {
        match check_compatibility(">=1.0.0 <2.0.0", "1.5.0") {
            CompatibilityResult::Compatible => {}
            other => panic!("expected Compatible, got {other:?}"),
        }
    }

    #[test]
    fn bounded_range_upper_incompatible() {
        match check_compatibility(">=1.0.0 <2.0.0", "2.0.0") {
            CompatibilityResult::Incompatible(_) => {}
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    #[test]
    fn unparseable_range_is_degraded() {
        match check_compatibility("foo bar", "1.0.0") {
            CompatibilityResult::Degraded(_) => {}
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    // -- Category filtering --------------------------------------------------

    #[test]
    fn category_display() {
        assert_eq!(ContributionCategory::AgentDefinition.to_string(), "agent_definition");
        assert_eq!(ContributionCategory::Plugin.to_string(), "plugin");
    }
}
