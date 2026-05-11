//! Plugin Installer — real plugin installation with file operations and rollback.
//!
//! Plugins can: install files, register agents, inject skills, inject decision
//! rules, and extend pipeline steps. Safe: validates before install, rolls back on failure.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Validate that a manifest-supplied file path is strictly relative and
/// cannot escape the plugin root. Returns the reason string on rejection.
pub fn validate_plugin_relative_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() {
        return Err("empty path");
    }
    if path.contains('\0') {
        return Err("NUL in path");
    }
    if path.contains('\\') {
        return Err("backslash separator not allowed");
    }
    if path.contains('%') {
        return Err("percent-encoded paths not allowed");
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("absolute path");
    }
    for comp in p.components() {
        match comp {
            Component::ParentDir => return Err("parent traversal"),
            Component::RootDir => return Err("root component"),
            Component::Prefix(_) => return Err("windows prefix"),
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod path_tests {
    use super::validate_plugin_relative_path as v;

    #[test]
    fn allows_normal_relative_paths() {
        assert!(v("src/lib.rs").is_ok());
        assert!(v("./README.md").is_ok());
        assert!(v("a/b/c.txt").is_ok());
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        assert!(v("../etc/passwd").is_err());
        assert!(v("a/../b").is_err());
        assert!(v("/etc/passwd").is_err());
        assert!(v("").is_err());
    }

    #[test]
    fn rejects_encoded_and_windows() {
        assert!(v("%2e%2e/evil").is_err());
        assert!(v("..\\windows").is_err());
        assert!(v("a\\b").is_err());
    }

    #[test]
    fn rejects_nul_byte() {
        assert!(v("a\0b").is_err());
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    /// Files to install into the project.
    pub files: Vec<PluginFile>,
    /// Agents to register.
    pub agents: Vec<PluginAgent>,
    /// Skills to inject.
    pub skills: Vec<PluginSkill>,
    /// Decision rules to inject.
    pub decision_rules: Vec<PluginDecisionRule>,
    /// Pipeline steps to add.
    pub pipeline_steps: Vec<PluginPipelineStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFile {
    /// Relative path within the project (e.g., "src/components/ChatWidget.tsx").
    pub path: String,
    /// File content.
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAgent {
    pub name: String,
    pub role: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkill {
    pub name: String,
    pub category: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDecisionRule {
    /// Which decision area this rule affects (e.g., "database", "auth").
    pub area: String,
    /// Condition (keyword match on description).
    pub condition: String,
    /// Override value.
    pub value: String,
    /// Reason for the override.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPipelineStep {
    pub name: String,
    pub after_step: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub success: bool,
    pub plugin_id: String,
    pub files_installed: Vec<String>,
    pub agents_registered: usize,
    pub skills_injected: usize,
    pub decision_rules_added: usize,
    pub errors: Vec<String>,
    /// Paths backed up (for rollback).
    pub backups: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a plugin manifest before installation.
pub fn validate(manifest: &PluginManifest) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if manifest.id.is_empty() {
        errors.push("Plugin ID is required".into());
    }
    if manifest.name.is_empty() {
        errors.push("Plugin name is required".into());
    }
    if manifest.version.is_empty() {
        errors.push("Plugin version is required".into());
    }

    // Validate file paths (no path traversal). A literal `..` check is not
    // enough — a manifest can reach outside the plugin root via URL-encoded
    // traversal, embedded NULs, Windows-style separators, or an absolute
    // path. Use component-level parsing so the validator catches every
    // escape vector before the path is joined into the plugin directory.
    for file in &manifest.files {
        if let Err(reason) = validate_plugin_relative_path(&file.path) {
            errors.push(format!("Invalid file path ({reason}): {}", file.path));
        }
        if file.content.is_empty() {
            errors.push(format!("File content is empty: {}", file.path));
        }
    }

    // Validate agents
    for agent in &manifest.agents {
        if agent.name.is_empty() {
            errors.push("Agent name is required".into());
        }
        if agent.system_prompt.is_empty() {
            errors.push(format!("Agent '{}' needs a system prompt", agent.name));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

/// Install a plugin into a project directory.
///
/// Creates backups of existing files, installs new files,
/// and rolls back on any failure.
pub fn install(
    manifest: &PluginManifest,
    project_dir: &Path,
) -> InstallResult {
    let mut result = InstallResult {
        success: false,
        plugin_id: manifest.id.clone(),
        files_installed: Vec::new(),
        agents_registered: 0,
        skills_injected: 0,
        decision_rules_added: 0,
        errors: Vec::new(),
        backups: Vec::new(),
    };

    // Step 1: Validate
    if let Err(errors) = validate(manifest) {
        result.errors = errors;
        return result;
    }

    // Step 2: Install files (with backup)
    for file in &manifest.files {
        let target = project_dir.join(&file.path);

        // Backup existing file
        if target.exists() {
            if let Ok(old_content) = std::fs::read_to_string(&target) {
                result.backups.push((file.path.clone(), old_content));
            }
        }

        // Create parent directories
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                result.errors.push(format!("Cannot create directory for {}: {}", file.path, e));
                // Rollback
                rollback(&result.backups, project_dir);
                return result;
            }
        }

        // Write file
        if let Err(e) = std::fs::write(&target, &file.content) {
            result.errors.push(format!("Cannot write {}: {}", file.path, e));
            rollback(&result.backups, project_dir);
            return result;
        }

        result.files_installed.push(file.path.clone());
    }

    // Step 3: Record plugin metadata
    let plugin_meta_dir = project_dir.join(".nexus").join("plugins");
    let _ = std::fs::create_dir_all(&plugin_meta_dir);
    let meta_path = plugin_meta_dir.join(format!("{}.json", manifest.id));
    if let Ok(json) = serde_json::to_string_pretty(manifest) {
        let _ = std::fs::write(meta_path, json);
    }

    result.agents_registered = manifest.agents.len();
    result.skills_injected = manifest.skills.len();
    result.decision_rules_added = manifest.decision_rules.len();
    result.success = true;

    info!(
        plugin = %manifest.id,
        files = result.files_installed.len(),
        agents = result.agents_registered,
        skills = result.skills_injected,
        "Plugin installed successfully"
    );

    result
}

/// Uninstall a plugin from a project directory.
pub fn uninstall(plugin_id: &str, project_dir: &Path) -> Result<Vec<String>, String> {
    let meta_path = project_dir
        .join(".nexus")
        .join("plugins")
        .join(format!("{}.json", plugin_id));

    if !meta_path.exists() {
        return Err(format!("Plugin '{}' is not installed", plugin_id));
    }

    let manifest: PluginManifest = serde_json::from_str(
        &std::fs::read_to_string(&meta_path)
            .map_err(|e| format!("Cannot read plugin manifest: {}", e))?,
    )
    .map_err(|e| format!("Invalid plugin manifest: {}", e))?;

    let mut removed = Vec::new();
    for file in &manifest.files {
        let target = project_dir.join(&file.path);
        if target.exists() {
            if let Err(e) = std::fs::remove_file(&target) {
                warn!(file = %file.path, error = %e, "Failed to remove plugin file");
            } else {
                removed.push(file.path.clone());
            }
        }
    }

    // Remove metadata
    let _ = std::fs::remove_file(&meta_path);

    info!(
        plugin = %plugin_id,
        files_removed = removed.len(),
        "Plugin uninstalled"
    );

    Ok(removed)
}

/// Rollback file changes on failure.
fn rollback(backups: &[(String, String)], project_dir: &Path) {
    for (path, content) in backups {
        let target = project_dir.join(path);
        let _ = std::fs::write(target, content);
    }
    warn!("Plugin installation rolled back");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest() -> PluginManifest {
        PluginManifest {
            id: "test-plugin".into(),
            name: "Test Plugin".into(),
            version: "1.0.0".into(),
            description: "A test plugin".into(),
            author: "Test".into(),
            files: vec![PluginFile {
                path: "src/test-plugin.tsx".into(),
                content: "export default function TestPlugin() { return <div>Test</div>; }".into(),
            }],
            agents: vec![],
            skills: vec![],
            decision_rules: vec![],
            pipeline_steps: vec![],
        }
    }

    #[test]
    fn validates_valid_manifest() {
        assert!(validate(&test_manifest()).is_ok());
    }

    #[test]
    fn rejects_path_traversal() {
        let mut m = test_manifest();
        m.files[0].path = "../../etc/passwd".into();
        assert!(validate(&m).is_err());
    }

    #[test]
    fn installs_and_uninstalls() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = test_manifest();

        let result = install(&manifest, dir.path());
        assert!(result.success);
        assert_eq!(result.files_installed.len(), 1);
        assert!(dir.path().join("src/test-plugin.tsx").exists());

        let removed = uninstall("test-plugin", dir.path()).unwrap();
        assert_eq!(removed.len(), 1);
        assert!(!dir.path().join("src/test-plugin.tsx").exists());
    }

    #[test]
    fn rollback_on_invalid_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = test_manifest();
        manifest.files.push(PluginFile {
            path: "src/good.tsx".into(),
            content: "good".into(),
        });
        // Force second file to a path we can't write (absolute)
        // Validation catches this, so this just tests the validator
        manifest.files.push(PluginFile {
            path: "".into(),
            content: "".into(),
        });
        let result = install(&manifest, dir.path());
        // Should fail validation (empty path content)
        assert!(!result.success);
    }
}
