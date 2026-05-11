//! Incremental Mutation Engine — change one thing without full rebuild.
//!
//! Instead of regenerating the entire app, this engine:
//! 1. Takes a natural language change request
//! 2. Identifies which file(s) need to change
//! 3. Generates ONLY the diff
//! 4. Validates the change
//! 5. Applies it
//! 6. Hot-reloads (Next.js dev server auto-detects file changes)

use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationRequest {
    pub change: String,          // "make the hero button red"
    pub target_file: Option<String>, // optional: user can specify which file
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationResult {
    pub files_changed: Vec<FileChange>,
    pub validation: MutationValidation,
    pub applied: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: String,    // "edit", "create", "delete"
    pub old_content: Option<String>,  // for rollback
    pub new_content: String,
    pub diff_summary: String,   // human-readable diff
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationValidation {
    pub valid: bool,
    pub warnings: Vec<String>,
}

/// Apply an incremental mutation to a project.
pub async fn mutate(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    request: &MutationRequest,
) -> Result<MutationResult, String> {
    let start = std::time::Instant::now();

    // Step 1: Identify target files
    let target_files = if let Some(ref target) = request.target_file {
        vec![target.clone()]
    } else {
        identify_target_files(project_dir, &request.change)
    };

    if target_files.is_empty() {
        return Err("Could not identify which files to change. Please specify a target file.".into());
    }

    // Step 2: Read current content of target files
    let mut file_contexts = Vec::new();
    for file in &target_files {
        let full_path = project_dir.join(file);
        if full_path.exists() {
            let content = std::fs::read_to_string(&full_path)
                .map_err(|e| format!("Cannot read {}: {}", file, e))?;
            file_contexts.push((file.clone(), content));
        }
    }

    // Step 3: Ask LLM to generate the change (focused, minimal prompt)
    let files_desc = file_contexts.iter().map(|(path, content)| {
        // Only send first 3000 chars of each file to save tokens
        let preview = content.chars().take(3000).collect::<String>();
        format!("=== FILE: {} ===\n{}\n=== END FILE ===", path, preview)
    }).collect::<Vec<_>>().join("\n\n");

    let prompt = format!(
        r#"Make this EXACT change to the code below:

Change: {}

Current files:
{}

Rules:
- Output ONLY the modified file(s) using the === FILE: path === format
- Make the MINIMUM change needed
- Do NOT rewrite the entire file — only change what's necessary
- If the change is CSS/style related, only modify the relevant classes
- Preserve all existing functionality

=== FILE: path ===
(complete modified file content)
=== END FILE ==="#,
        request.change, files_desc
    );

    let response = super::handlers::chat::call_llm_simple_for_project(app, &prompt, Some(project_id))
        .await
        .map_err(|e| format!("LLM call failed: {}", e))?;

    let generated_files = nexus_store::parse_file_blocks(&response);

    if generated_files.is_empty() {
        return Err("LLM did not generate any file changes".into());
    }

    // Step 4: Build file changes with diff
    let mut changes = Vec::new();
    for (path, new_content) in &generated_files {
        let old_content = file_contexts.iter()
            .find(|(p, _)| p == path)
            .map(|(_, c)| c.clone());

        let diff_summary = if let Some(ref old) = old_content {
            compute_diff_summary(old, new_content)
        } else {
            format!("New file: {} lines", new_content.lines().count())
        };

        changes.push(FileChange {
            path: path.clone(),
            change_type: if old_content.is_some() { "edit" } else { "create" }.into(),
            old_content,
            new_content: new_content.clone(),
            diff_summary,
        });
    }

    // Step 5: Validate
    let mut warnings = Vec::new();
    for change in &changes {
        if change.new_content.contains("TODO") || change.new_content.contains("FIXME") {
            warnings.push(format!("{}: contains TODO/FIXME", change.path));
        }
        if change.new_content.is_empty() {
            warnings.push(format!("{}: file is empty after change", change.path));
        }
    }

    let validation = MutationValidation {
        valid: !changes.is_empty(),
        warnings,
    };

    // Step 6: Apply changes (with backup)
    let backup_dir = project_dir.join(".nexus").join("backups").join("mutation");
    let _ = std::fs::create_dir_all(&backup_dir);

    for change in &changes {
        let full_path = project_dir.join(&change.path);

        // Backup
        if let Some(ref old) = change.old_content {
            let backup_path = backup_dir.join(&change.path);
            if let Some(parent) = backup_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&backup_path, old);
        }

        // Write new content
        if let Some(parent) = full_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&full_path, &change.new_content)
            .map_err(|e| format!("Failed to write {}: {}", change.path, e))?;
    }

    // Next.js dev server will auto-detect the file change and hot-reload

    let duration = start.elapsed().as_millis() as u64;

    Ok(MutationResult {
        files_changed: changes,
        validation,
        applied: true,
        duration_ms: duration,
    })
}

/// Identify which files need to change based on the request.
fn identify_target_files(project_dir: &Path, change: &str) -> Vec<String> {
    let lower = change.to_lowercase();
    let mut targets = Vec::new();

    // UI changes -> find page/component files
    let ui_keywords = ["button", "color", "text", "font", "style", "layout", "header",
        "footer", "nav", "hero", "section", "image", "logo", "title", "background",
        "padding", "margin", "border", "shadow", "animation", "hover", "dark", "light"];

    let api_keywords = ["api", "endpoint", "route", "handler", "database", "query",
        "fetch", "post", "get", "delete", "update"];

    let is_ui_change = ui_keywords.iter().any(|k| lower.contains(k));
    let is_api_change = api_keywords.iter().any(|k| lower.contains(k));

    // Walk project files
    fn walk(dir: &Path, root: &Path, targets: &mut Vec<String>, is_ui: bool, is_api: bool) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || matches!(name.as_str(), "node_modules" | ".next" | "target") { continue; }
            let path = entry.path();
            if path.is_dir() { walk(&path, root, targets, is_ui, is_api); continue; }

            let rel = path.strip_prefix(root).unwrap_or(&path).display().to_string();

            let include = if is_ui {
                rel.ends_with("page.tsx") || rel.ends_with("page.ts") || rel.contains("components/")
            } else if is_api {
                rel.contains("/api/") && rel.ends_with("route.ts")
            } else {
                false
            };
            if include {
                targets.push(rel);
            } else if !is_ui && !is_api {
                // Generic: include main pages
                if rel.ends_with("page.tsx") || rel.ends_with("page.ts") {
                    targets.push(rel);
                }
            }
        }
    }

    walk(project_dir, project_dir, &mut targets, is_ui_change, is_api_change);

    // Limit to most relevant files (max 3)
    targets.truncate(3);
    targets
}

/// Compute a human-readable diff summary
fn compute_diff_summary(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let added = new_lines.len() as i32 - old_lines.len() as i32;
    let changed = old_lines.iter().zip(new_lines.iter())
        .filter(|(a, b)| a != b)
        .count();

    if added > 0 {
        format!("+{} lines, {} lines changed", added, changed)
    } else if added < 0 {
        format!("{} lines removed, {} lines changed", -added, changed)
    } else {
        format!("{} lines changed", changed)
    }
}
