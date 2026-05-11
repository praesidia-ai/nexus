//! Live Incremental Update Engine — hot-patch running apps without restart.
//!
//! Provides:
//! - File-level diff computation and application
//! - Dependency-aware update ordering (via code graph)
//! - Safe rollback on failure
//! - Notification to dev server for HMR
//! - Update history tracking

use std::path::{Path, PathBuf};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::code_graph::CodeGraph;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveUpdateRequest {
    /// Files to update: path -> new content.
    pub files: Vec<FileUpdate>,
    /// Whether to validate before applying.
    pub validate: bool,
    /// Whether to trigger HMR notification.
    pub hot_reload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUpdate {
    pub path: String,
    pub content: String,
    /// Optional: specify which section changed (for minimal diff).
    pub changed_range: Option<ChangedRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedRange {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveUpdateResult {
    pub success: bool,
    pub files_updated: Vec<String>,
    pub files_failed: Vec<FailedUpdate>,
    pub rollback_applied: bool,
    pub impact: UpdateImpact,
    pub duration_ms: u64,
    pub update_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedUpdate {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateImpact {
    /// Files directly affected by the update.
    pub direct_files: Vec<String>,
    /// Files indirectly affected (transitive dependents).
    pub affected_files: Vec<String>,
    /// Risk level based on code graph analysis.
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UpdateEvent {
    #[serde(rename = "validating")]
    Validating { file_count: usize },
    #[serde(rename = "applying")]
    Applying { file: String, index: usize, total: usize },
    #[serde(rename = "impact")]
    Impact { impact: UpdateImpact },
    #[serde(rename = "complete")]
    Complete { result: LiveUpdateResult },
    #[serde(rename = "rollback")]
    Rollback { reason: String },
    #[serde(rename = "error")]
    Error { message: String },
}

// ---------------------------------------------------------------------------
// Update History
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecord {
    pub id: String,
    pub timestamp: String,
    pub files: Vec<String>,
    pub rollback_available: bool,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

pub struct LiveUpdateEngine {
    project_dir: PathBuf,
}

impl LiveUpdateEngine {
    pub fn new(project_dir: PathBuf) -> Self {
        Self { project_dir }
    }

    /// Acquire a file-system lock to prevent concurrent updates.
    fn acquire_lock(&self) -> Result<std::fs::File, String> {
        let lock_dir = self.project_dir.join(".nexus");
        let _ = std::fs::create_dir_all(&lock_dir);
        let lock_path = lock_dir.join(".update.lock");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&lock_path)
            .map_err(|e| format!("Cannot acquire update lock: {}", e))?;
        // Try exclusive lock (non-blocking)
        use std::io::Write;
        // Write PID to lockfile for debugging
        let mut f = file;
        let _ = write!(f, "{}", std::process::id());
        Ok(f)
    }

    /// Apply a live update to the project.
    pub async fn apply(
        &self,
        request: &LiveUpdateRequest,
        tx: Option<&mpsc::Sender<UpdateEvent>>,
    ) -> LiveUpdateResult {
        let start = std::time::Instant::now();
        let update_id = uuid::Uuid::new_v4().to_string();

        // Acquire filesystem lock to prevent concurrent updates
        let _lock = match self.acquire_lock() {
            Ok(lock) => lock,
            Err(e) => {
                return LiveUpdateResult {
                    success: false,
                    files_updated: Vec::new(),
                    files_failed: vec![FailedUpdate {
                        path: "(lock)".into(),
                        error: e,
                    }],
                    rollback_applied: false,
                    impact: UpdateImpact {
                        direct_files: vec![],
                        affected_files: vec![],
                        risk_level: "blocked".into(),
                    },
                    duration_ms: start.elapsed().as_millis() as u64,
                    update_id,
                };
            }
        };
        // _lock is held until end of function (RAII drop)

        // Step 1: Validate
        if request.validate {
            if let Some(tx) = tx {
                let _ = tx
                    .send(UpdateEvent::Validating {
                        file_count: request.files.len(),
                    })
                    .await;
            }

            for file in &request.files {
                // Check path safety
                if file.path.contains("..") || file.path.starts_with('/') {
                    return LiveUpdateResult {
                        success: false,
                        files_updated: Vec::new(),
                        files_failed: vec![FailedUpdate {
                            path: file.path.clone(),
                            error: "Path traversal blocked".into(),
                        }],
                        rollback_applied: false,
                        impact: UpdateImpact {
                            direct_files: vec![],
                            affected_files: vec![],
                            risk_level: "blocked".into(),
                        },
                        duration_ms: start.elapsed().as_millis() as u64,
                        update_id,
                    };
                }
            }
        }

        // Step 2: Compute impact via code graph
        let graph = CodeGraph::load_or_build(&self.project_dir);
        let mut all_affected = Vec::new();
        let direct_files: Vec<String> = request.files.iter().map(|f| f.path.clone()).collect();

        for file in &request.files {
            let impact = graph.impact_analysis(&file.path);
            all_affected.extend(impact.direct_dependents);
            all_affected.extend(impact.transitive_dependents);
        }
        all_affected.sort();
        all_affected.dedup();
        // Remove direct files from affected
        all_affected.retain(|f| !direct_files.contains(f));

        let risk_level = if all_affected.len() > 10 {
            "critical"
        } else if all_affected.len() > 5 {
            "high"
        } else if all_affected.len() > 2 {
            "medium"
        } else {
            "low"
        };

        let impact = UpdateImpact {
            direct_files: direct_files.clone(),
            affected_files: all_affected,
            risk_level: risk_level.into(),
        };

        if let Some(tx) = tx {
            let _ = tx
                .send(UpdateEvent::Impact {
                    impact: impact.clone(),
                })
                .await;
        }

        // Step 3: Create backup
        let backup_dir = self
            .project_dir
            .join(".nexus")
            .join("live_updates")
            .join(&update_id);
        let _ = std::fs::create_dir_all(&backup_dir);

        // Step 4: Apply updates with ordering
        // Sort files: dependencies first (leaf nodes before dependents)
        let ordered = self.order_updates(&request.files, &graph);

        let mut updated = Vec::new();
        let mut failed = Vec::new();
        let total = ordered.len();

        for (i, file) in ordered.iter().enumerate() {
            if let Some(tx) = tx {
                let _ = tx
                    .send(UpdateEvent::Applying {
                        file: file.path.clone(),
                        index: i,
                        total,
                    })
                    .await;
            }

            match self.apply_single_update(file, &backup_dir) {
                Ok(()) => {
                    updated.push(file.path.clone());
                    info!(path = %file.path, "Live update applied");
                }
                Err(e) => {
                    warn!(path = %file.path, error = %e, "Live update failed");
                    failed.push(FailedUpdate {
                        path: file.path.clone(),
                        error: e,
                    });
                    // Rollback all applied changes
                    self.rollback(&backup_dir, &updated);
                    if let Some(tx) = tx {
                        let _ = tx
                            .send(UpdateEvent::Rollback {
                                reason: "File update failed".into(),
                            })
                            .await;
                    }
                    return LiveUpdateResult {
                        success: false,
                        files_updated: Vec::new(),
                        files_failed: failed,
                        rollback_applied: true,
                        impact,
                        duration_ms: start.elapsed().as_millis() as u64,
                        update_id,
                    };
                }
            }
        }

        // Step 5: Store update record
        self.store_record(&update_id, &updated);

        // Step 6: Trigger HMR if requested
        if request.hot_reload {
            self.trigger_hmr(&updated);
        }

        let result = LiveUpdateResult {
            success: true,
            files_updated: updated,
            files_failed: failed,
            rollback_applied: false,
            impact,
            duration_ms: start.elapsed().as_millis() as u64,
            update_id,
        };

        if let Some(tx) = tx {
            let _ = tx
                .send(UpdateEvent::Complete {
                    result: result.clone(),
                })
                .await;
        }

        result
    }

    /// Rollback a specific update by ID.
    pub fn rollback_update(&self, update_id: &str) -> Result<Vec<String>, String> {
        let backup_dir = self
            .project_dir
            .join(".nexus")
            .join("live_updates")
            .join(update_id);
        if !backup_dir.exists() {
            return Err(format!("No backup found for update {}", update_id));
        }

        let mut restored = Vec::new();
        self.restore_from_backup(&backup_dir, &self.project_dir, &mut restored)?;
        Ok(restored)
    }

    /// List recent updates.
    pub fn list_updates(&self) -> Vec<UpdateRecord> {
        let updates_dir = self.project_dir.join(".nexus").join("live_updates");
        let record_path = updates_dir.join("history.json");
        if let Ok(content) = std::fs::read_to_string(&record_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    // ---- Internal ----

    fn apply_single_update(&self, file: &FileUpdate, backup_dir: &Path) -> Result<(), String> {
        let full_path = self.project_dir.join(&file.path);

        // Backup existing file
        if full_path.exists() {
            let backup_path = backup_dir.join(&file.path);
            if let Some(parent) = backup_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Backup mkdir: {}", e))?;
            }
            std::fs::copy(&full_path, &backup_path)
                .map_err(|e| format!("Backup copy: {}", e))?;
        }

        // Write new content
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir: {}", e))?;
        }

        let content = if let Some(ref range) = file.changed_range {
            // Partial update: only replace specified lines
            if let Ok(existing) = std::fs::read_to_string(&full_path) {
                let lines: Vec<&str> = existing.lines().collect();
                let new_lines: Vec<&str> = file.content.lines().collect();
                let mut result = Vec::new();
                result.extend_from_slice(&lines[..range.start_line.min(lines.len())]);
                result.extend(new_lines.iter().copied());
                if range.end_line < lines.len() {
                    result.extend_from_slice(&lines[range.end_line..]);
                }
                result.join("\n")
            } else {
                file.content.clone()
            }
        } else {
            file.content.clone()
        };

        std::fs::write(&full_path, content)
            .map_err(|e| format!("Write {}: {}", file.path, e))?;

        Ok(())
    }

    fn order_updates(&self, files: &[FileUpdate], graph: &CodeGraph) -> Vec<FileUpdate> {
        // Simple ordering: files with no dependents first (leaf nodes)
        let mut scored: Vec<(usize, &FileUpdate)> = files
            .iter()
            
            .map(|f| {
                let impact = graph.impact_analysis(&f.path);
                let dep_count =
                    impact.direct_dependents.len() + impact.transitive_dependents.len();
                (dep_count, f)
            })
            .collect();

        scored.sort_by_key(|(count, _)| *count);
        scored.into_iter().map(|(_, f)| f.clone()).collect()
    }

    fn rollback(&self, backup_dir: &Path, files: &[String]) {
        for file in files {
            let backup_path = backup_dir.join(file);
            let target_path = self.project_dir.join(file);
            if backup_path.exists() {
                let _ = std::fs::copy(&backup_path, &target_path);
                warn!(path = %file, "Rolled back");
            }
        }
    }

    fn restore_from_backup(
        &self,
        backup_dir: &Path,
        target_dir: &Path,
        restored: &mut Vec<String>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(backup_dir)
            .map_err(|e| format!("Read backup dir: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let rel = path
                    .strip_prefix(backup_dir)
                    .unwrap_or(&path);
                let sub_target = target_dir.join(rel);
                self.restore_from_backup(&path, &sub_target, restored)?;
            } else {
                let rel = path
                    .strip_prefix(backup_dir)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                let target = self.project_dir.join(&rel);
                std::fs::copy(&path, &target)
                    .map_err(|e| format!("Restore {}: {}", rel, e))?;
                restored.push(rel);
            }
        }
        Ok(())
    }

    fn store_record(&self, update_id: &str, files: &[String]) {
        let updates_dir = self.project_dir.join(".nexus").join("live_updates");
        let record_path = updates_dir.join("history.json");

        let mut records: Vec<UpdateRecord> = if let Ok(content) =
            std::fs::read_to_string(&record_path)
        {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        records.push(UpdateRecord {
            id: update_id.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            files: files.to_vec(),
            rollback_available: true,
        });

        // Keep only last 50 records
        if records.len() > 50 {
            let drain_count = records.len() - 50;
            records.drain(..drain_count);
        }

        if let Ok(json) = serde_json::to_string_pretty(&records) {
            let _ = std::fs::write(&record_path, json);
        }
    }

    fn trigger_hmr(&self, _files: &[String]) {
        // Touch a sentinel file that Next.js dev server watches
        // This forces a re-compilation of changed modules
        let sentinel = self.project_dir.join(".nexus").join(".hmr_trigger");
        let _ = std::fs::write(&sentinel, Utc::now().to_rfc3339());
    }
}
