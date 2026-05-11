//! Continuous Improvement Loop — auto-apply safe upgrades after generation.
//!
//! After the pipeline completes and post-build analysis runs, this module
//! picks 1–2 safe, auto-fixable improvements and applies them immediately.
//!
//! Safety rules:
//! - Only applies suggestions marked `auto_fixable: true`
//! - Only applies `Effort::Trivial` suggestions
//! - Maximum 2 improvements per cycle
//! - Generates a diff summary for transparency
//! - Uses deterministic templates (no LLM for auto-fixes)

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::post_build_intel::{Effort, PostBuildAnalysis, Suggestion};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Result of running one improvement cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementResult {
    /// Improvements that were applied.
    pub applied: Vec<AppliedImprovement>,
    /// Improvements that were skipped (too risky, too complex, etc.).
    pub skipped: Vec<SkippedImprovement>,
    /// Summary for the user.
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedImprovement {
    pub suggestion_id: String,
    pub title: String,
    pub files_modified: Vec<String>,
    pub diff_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedImprovement {
    pub suggestion_id: String,
    pub title: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

const MAX_AUTO_IMPROVEMENTS: usize = 2;

/// Run the improvement loop on a generated project.
/// Returns which improvements were applied and which were skipped.
pub fn improve(project_dir: &Path, analysis: &PostBuildAnalysis) -> ImprovementResult {
    let all_suggestions: Vec<&Suggestion> = analysis
        .missing_features
        .iter()
        .chain(analysis.ux_improvements.iter())
        .chain(analysis.performance_issues.iter())
        .collect();

    // Filter to auto-fixable trivial improvements
    let candidates: Vec<&Suggestion> = all_suggestions
        .into_iter()
        .filter(|s| s.auto_fixable && matches!(s.effort, Effort::Trivial))
        .collect();

    let mut applied = Vec::new();
    let mut skipped = Vec::new();

    for suggestion in candidates {
        if applied.len() >= MAX_AUTO_IMPROVEMENTS {
            skipped.push(SkippedImprovement {
                suggestion_id: suggestion.id.clone(),
                title: suggestion.title.clone(),
                reason: "Maximum auto-improvements reached for this cycle".into(),
            });
            continue;
        }

        match apply_improvement(project_dir, suggestion) {
            Ok(result) => applied.push(result),
            Err(reason) => {
                skipped.push(SkippedImprovement {
                    suggestion_id: suggestion.id.clone(),
                    title: suggestion.title.clone(),
                    reason,
                });
            }
        }
    }

    let summary = if applied.is_empty() {
        "No auto-improvements needed — your app looks great!".into()
    } else {
        format!(
            "Applied {} improvement{}: {}",
            applied.len(),
            if applied.len() == 1 { "" } else { "s" },
            applied
                .iter()
                .map(|a| a.title.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    ImprovementResult {
        applied,
        skipped,
        summary,
    }
}

// ---------------------------------------------------------------------------
// Individual improvement applicators
// ---------------------------------------------------------------------------

fn apply_improvement(
    project_dir: &Path,
    suggestion: &Suggestion,
) -> Result<AppliedImprovement, String> {
    match suggestion.id.as_str() {
        "missing_404" => apply_not_found_page(project_dir, suggestion),
        "missing_loading" => apply_loading_page(project_dir, suggestion),
        "missing_error_boundary" => apply_error_boundary(project_dir, suggestion),
        "missing_favicon" => {
            // Can't generate a real favicon deterministically — skip
            Err("Favicon generation requires design context".into())
        }
        "ux_animations" => apply_entrance_animations(project_dir, suggestion),
        id if id.starts_with("perf_unoptimized_img_") => {
            apply_image_optimization(project_dir, suggestion)
        }
        id if id.starts_with("perf_inline_data_") => {
            // Extracting data requires understanding the structure — skip
            Err("Data extraction requires semantic understanding".into())
        }
        _ => Err(format!("No auto-fix template for: {}", suggestion.id)),
    }
}

fn apply_not_found_page(
    project_dir: &Path,
    suggestion: &Suggestion,
) -> Result<AppliedImprovement, String> {
    let path = project_dir.join("app/not-found.tsx");
    if path.exists() {
        return Err("not-found.tsx already exists".into());
    }

    let content = r#"import Link from "next/link";

export default function NotFound() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-4">
      <h1 className="text-4xl font-bold">404</h1>
      <p className="text-muted-foreground">This page could not be found.</p>
      <Link
        href="/"
        className="rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:opacity-90"
      >
        Go home
      </Link>
    </div>
  );
}
"#;

    write_file(&path, content)?;

    Ok(AppliedImprovement {
        suggestion_id: suggestion.id.clone(),
        title: suggestion.title.clone(),
        files_modified: vec!["app/not-found.tsx".into()],
        diff_summary: "Created custom 404 page with back-to-home link".into(),
    })
}

fn apply_loading_page(
    project_dir: &Path,
    suggestion: &Suggestion,
) -> Result<AppliedImprovement, String> {
    let path = project_dir.join("app/loading.tsx");
    if path.exists() {
        return Err("loading.tsx already exists".into());
    }

    let content = r#"export default function Loading() {
  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="flex flex-col items-center gap-3">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
        <p className="text-sm text-muted-foreground">Loading...</p>
      </div>
    </div>
  );
}
"#;

    write_file(&path, content)?;

    Ok(AppliedImprovement {
        suggestion_id: suggestion.id.clone(),
        title: suggestion.title.clone(),
        files_modified: vec!["app/loading.tsx".into()],
        diff_summary: "Created loading skeleton with spinner".into(),
    })
}

fn apply_error_boundary(
    project_dir: &Path,
    suggestion: &Suggestion,
) -> Result<AppliedImprovement, String> {
    let path = project_dir.join("app/error.tsx");
    if path.exists() {
        return Err("error.tsx already exists".into());
    }

    let content = r#""use client";

export default function Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-4">
      <h2 className="text-2xl font-bold">Something went wrong</h2>
      <p className="text-sm text-muted-foreground max-w-md text-center">
        An unexpected error occurred. Please try again.
      </p>
      <button
        onClick={reset}
        className="rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:opacity-90"
      >
        Try again
      </button>
    </div>
  );
}
"#;

    write_file(&path, content)?;

    Ok(AppliedImprovement {
        suggestion_id: suggestion.id.clone(),
        title: suggestion.title.clone(),
        files_modified: vec!["app/error.tsx".into()],
        diff_summary: "Created error boundary with retry button".into(),
    })
}

fn apply_entrance_animations(
    project_dir: &Path,
    suggestion: &Suggestion,
) -> Result<AppliedImprovement, String> {
    // Add animation utility classes to globals.css
    let globals_path = project_dir.join("app/globals.css");
    if !globals_path.exists() {
        return Err("globals.css not found".into());
    }

    let existing = std::fs::read_to_string(&globals_path)
        .map_err(|e| format!("Read globals.css: {}", e))?;

    if existing.contains("animate-fade-up") {
        return Err("Animations already present in globals.css".into());
    }

    let animation_block = r#"
/* Entrance animations */
@keyframes fade-up {
  from { opacity: 0; transform: translateY(12px); }
  to { opacity: 1; transform: translateY(0); }
}
@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}
.animate-fade-up { animation: fade-up 0.5s ease-out both; }
.animate-fade-in { animation: fade-in 0.4s ease-out both; }
.animate-delay-100 { animation-delay: 100ms; }
.animate-delay-200 { animation-delay: 200ms; }
.animate-delay-300 { animation-delay: 300ms; }
"#;

    let updated = format!("{}\n{}", existing.trim(), animation_block);
    write_file(&globals_path, &updated)?;

    Ok(AppliedImprovement {
        suggestion_id: suggestion.id.clone(),
        title: suggestion.title.clone(),
        files_modified: vec!["app/globals.css".into()],
        diff_summary: "Added fade-up and fade-in animation utilities to globals.css".into(),
    })
}

fn apply_image_optimization(
    project_dir: &Path,
    suggestion: &Suggestion,
) -> Result<AppliedImprovement, String> {
    if suggestion.affected_files.is_empty() {
        return Err("No affected files specified".into());
    }

    let file_path = project_dir.join(&suggestion.affected_files[0]);
    if !file_path.exists() {
        return Err(format!("{} not found", suggestion.affected_files[0]));
    }

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Read file: {}", e))?;

    // Simple replacement: <img src="..." /> → <Image src="..." width={800} height={600} />
    if !content.contains("<img ") {
        return Err("No <img> tags found".into());
    }

    let mut updated = content; // move, not clone

    // Add import if not present
    if !updated.contains("next/image") {
        // Insert after first import line
        if let Some(pos) = updated.find("import ") {
            if let Some(end) = updated[pos..].find('\n') {
                let insert_pos = pos + end + 1;
                updated.insert_str(insert_pos, "import Image from \"next/image\";\n");
            }
        }
    }

    // Replace <img with <Image (basic case only)
    updated = updated.replace("<img ", "<Image width={800} height={600} ");

    write_file(&file_path, &updated)?;

    Ok(AppliedImprovement {
        suggestion_id: suggestion.id.clone(),
        title: suggestion.title.clone(),
        files_modified: suggestion.affected_files.clone(),
        diff_summary: "Replaced <img> with next/image for automatic optimization".into(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {}", e))?;
    }
    std::fs::write(path, content).map_err(|e| format!("write: {}", e))
}
