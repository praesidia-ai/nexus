//! Perceived Speed Engine — optimistic UI updates, progressive rendering,
//! and latency-hiding techniques for the execution pipeline.
//!
//! Provides:
//! - Predictive step estimation (how long each pipeline step will take)
//! - Progressive event stream with optimistic previews
//! - Skeleton/placeholder generation while LLM processes
//! - Step parallelization hints
//! - Latency budgeting per step

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Estimated timeline for a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEstimate {
    pub total_estimated_ms: u64,
    pub steps: Vec<StepEstimate>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEstimate {
    pub name: String,
    pub estimated_ms: u64,
    pub can_parallel: bool,
    pub skeleton_available: bool,
    pub progressive_preview: bool,
}

/// Skeleton content for optimistic rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkeletonContent {
    pub file_path: String,
    pub skeleton_type: SkeletonType,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkeletonType {
    /// Page layout skeleton with placeholders.
    PageLayout,
    /// API route skeleton with basic structure.
    ApiRoute,
    /// Component skeleton with props interface.
    Component,
    /// Config file skeleton.
    Config,
}

/// Progressive preview event sent during generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SpeedEvent {
    #[serde(rename = "estimate")]
    Estimate { estimate: PipelineEstimate },
    #[serde(rename = "skeleton")]
    Skeleton { skeletons: Vec<SkeletonContent> },
    #[serde(rename = "preview")]
    Preview {
        step: String,
        preview: String,
        confidence: f64,
    },
    #[serde(rename = "progress")]
    Progress {
        step: String,
        pct: f64,
        elapsed_ms: u64,
        estimated_remaining_ms: u64,
    },
    #[serde(rename = "step_faster")]
    StepFaster {
        step: String,
        saved_ms: u64,
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// Historical Timing Data
// ---------------------------------------------------------------------------

/// Stores historical timing data for estimation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingHistory {
    pub step_timings: HashMap<String, Vec<u64>>,
}

impl TimingHistory {
    pub fn record(&mut self, step: &str, duration_ms: u64) {
        let entries = self.step_timings.entry(step.to_string()).or_default();
        entries.push(duration_ms);
        // Keep last 20 timings
        if entries.len() > 20 {
            entries.remove(0);
        }
    }

    pub fn estimate(&self, step: &str) -> u64 {
        if let Some(timings) = self.step_timings.get(step) {
            if timings.is_empty() {
                return default_step_estimate(step);
            }
            // Weighted average: recent timings count more
            let mut weighted_sum = 0u64;
            let mut weight_total = 0u64;
            for (i, &t) in timings.iter().enumerate() {
                let weight = (i + 1) as u64;
                weighted_sum += t * weight;
                weight_total += weight;
            }
            weighted_sum / weight_total
        } else {
            default_step_estimate(step)
        }
    }

    pub fn confidence(&self, step: &str) -> f64 {
        if let Some(timings) = self.step_timings.get(step) {
            if timings.len() < 3 {
                return 0.3;
            }
            let avg = timings.iter().sum::<u64>() as f64 / timings.len() as f64;
            let variance = timings
                .iter()
                .map(|&t| {
                    let diff = t as f64 - avg;
                    diff * diff
                })
                .sum::<f64>()
                / timings.len() as f64;
            let std_dev = variance.sqrt();
            let cv = if avg > 0.0 { std_dev / avg } else { 1.0 };
            // Lower coefficient of variation = higher confidence
            (1.0 - cv.min(1.0)).max(0.1)
        } else {
            0.2
        }
    }
}

fn default_step_estimate(step: &str) -> u64 {
    match step {
        "analyze_intent" => 100,
        "generate_spec" => 8_000,
        "validate_spec" => 2_000,
        "generate_schema" => 1_000,
        // Fused parallel step takes ~max(api, ui) instead of api+ui
        "generate_api_and_ui" => 15_000,
        // Individual steps kept for backwards compat
        "generate_api" => 10_000,
        "generate_ui" => 15_000,
        "generate_agents" => 5_000,
        "validate_files" => 3_000,
        "write_to_disk" => 500,
        "install_deps" => 30_000,
        "start_health" => 10_000,
        _ => 5_000,
    }
}

// ---------------------------------------------------------------------------
// Estimator
// ---------------------------------------------------------------------------

pub struct SpeedEstimator {
    history: TimingHistory,
}

impl SpeedEstimator {
    pub fn new(history: TimingHistory) -> Self {
        Self { history }
    }

    pub fn with_defaults() -> Self {
        Self::new(TimingHistory::default())
    }

    /// Load history from project directory.
    pub fn load(project_dir: &std::path::Path) -> Self {
        let path = project_dir.join(".nexus").join("timing_history.json");
        let history = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            TimingHistory::default()
        };
        Self { history }
    }

    /// Save history to project directory.
    pub fn save(&self, project_dir: &std::path::Path) {
        let dir = project_dir.join(".nexus");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string(&self.history) {
            let _ = std::fs::write(dir.join("timing_history.json"), json);
        }
    }

    /// Record a step timing.
    pub fn record_step(&mut self, step: &str, duration_ms: u64) {
        self.history.record(step, duration_ms);
    }

    /// Estimate the full pipeline timeline.
    pub fn estimate_pipeline(&self, steps: &[&str]) -> PipelineEstimate {
        let step_estimates: Vec<StepEstimate> = steps
            .iter()
            .map(|&step| {
                let estimated_ms = self.history.estimate(step);
                StepEstimate {
                    name: step.to_string(),
                    estimated_ms,
                    can_parallel: can_parallelize(step),
                    skeleton_available: has_skeleton(step),
                    progressive_preview: has_preview(step),
                }
            })
            .collect();

        // Calculate total considering parallelization
        let mut total = 0u64;
        let mut i = 0;
        while i < step_estimates.len() {
            if step_estimates[i].can_parallel && i + 1 < step_estimates.len() && step_estimates[i + 1].can_parallel {
                // Parallel steps: take max duration
                let max_dur = step_estimates[i]
                    .estimated_ms
                    .max(step_estimates[i + 1].estimated_ms);
                total += max_dur;
                i += 2;
            } else {
                total += step_estimates[i].estimated_ms;
                i += 1;
            }
        }

        let avg_confidence: f64 = steps
            .iter()
            .map(|s| self.history.confidence(s))
            .sum::<f64>()
            / steps.len() as f64;

        PipelineEstimate {
            total_estimated_ms: total,
            steps: step_estimates,
            confidence: avg_confidence,
        }
    }
}

fn can_parallelize(step: &str) -> bool {
    matches!(
        step,
        "generate_api" | "generate_ui" | "generate_agents" | "validate_files"
    )
}

fn has_skeleton(step: &str) -> bool {
    matches!(
        step,
        "generate_ui" | "generate_api" | "generate_schema"
    )
}

fn has_preview(step: &str) -> bool {
    matches!(
        step,
        "generate_spec" | "generate_ui" | "generate_api"
    )
}

// ---------------------------------------------------------------------------
// Skeleton Generator
// ---------------------------------------------------------------------------

/// Generate skeleton files for optimistic rendering.
pub fn generate_skeletons(app_type: &str, pages: &[String]) -> Vec<SkeletonContent> {
    let mut skeletons = Vec::new();

    // Layout skeleton
    skeletons.push(SkeletonContent {
        file_path: "src/app/layout.tsx".into(),
        skeleton_type: SkeletonType::PageLayout,
        content: format!(
            r#"import type {{ Metadata }} from 'next';
import './globals.css';

export const metadata: Metadata = {{
  title: '{}',
  description: 'Generated by Nexus',
}};

export default function RootLayout({{ children }}: {{ children: React.ReactNode }}) {{
  return (
    <html lang="en">
      <body>{{children}}</body>
    </html>
  );
}}"#,
            app_type
        ),
    });

    // Page skeletons
    for page in pages {
        let component_name = page
            .split('/')
            .next_back()
            .unwrap_or("Page")
            .replace('-', " ");
        let capitalized = component_name
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join("");

        skeletons.push(SkeletonContent {
            file_path: format!("src/app{}/page.tsx", page),
            skeleton_type: SkeletonType::PageLayout,
            content: format!(
                r#"export default function {}Page() {{
  return (
    <main className="container mx-auto px-4 py-8">
      <h1 className="text-3xl font-bold">{}</h1>
      {{/* Content loading... */}}
    </main>
  );
}}"#,
                capitalized, component_name
            ),
        });
    }

    // Config skeletons
    skeletons.push(SkeletonContent {
        file_path: "tailwind.config.ts".into(),
        skeleton_type: SkeletonType::Config,
        content: r#"import type { Config } from "tailwindcss";

const config: Config = {
  content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: { extend: {} },
  plugins: [],
};

export default config;"#
            .into(),
    });

    skeletons
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_estimates_are_reasonable() {
        let estimator = SpeedEstimator::with_defaults();
        let estimate = estimator.estimate_pipeline(&[
            "analyze_intent",
            "generate_spec",
            "generate_schema",
            "generate_api",
            "generate_ui",
            "write_to_disk",
        ]);

        assert!(estimate.total_estimated_ms > 0);
        assert!(estimate.total_estimated_ms < 120_000); // Under 2 minutes
        assert_eq!(estimate.steps.len(), 6);
    }

    #[test]
    fn timing_history_improves_estimates() {
        let mut estimator = SpeedEstimator::with_defaults();

        // Record some fast timings
        estimator.record_step("generate_spec", 3000);
        estimator.record_step("generate_spec", 2800);
        estimator.record_step("generate_spec", 3200);

        let estimate = estimator.estimate_pipeline(&["generate_spec"]);
        // Should be around 3000ms, not the default 8000ms
        assert!(estimate.steps[0].estimated_ms < 5000);
    }

    #[test]
    fn confidence_increases_with_data() {
        let mut history = TimingHistory::default();

        // No data -> low confidence
        assert!(history.confidence("generate_spec") < 0.3);

        // Add consistent data -> high confidence
        for _ in 0..10 {
            history.record("generate_spec", 3000);
        }
        assert!(history.confidence("generate_spec") > 0.8);
    }

    #[test]
    fn skeleton_generation() {
        let skeletons = generate_skeletons("SaaS App", &["/".into(), "/dashboard".into()]);
        assert!(skeletons.len() >= 3); // layout + 2 pages + config
        assert!(skeletons.iter().any(|s| s.file_path.contains("layout.tsx")));
    }
}
