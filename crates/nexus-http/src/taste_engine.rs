//! Taste Engine — Heuristic + Visual Scoring Pipeline.
//!
//! A stage-based, extensible scoring pipeline that analyzes generated projects
//! across multiple quality dimensions: static analysis, content quality, code
//! quality, and optional visual analysis (via multimodal LLM).
//!
//! Each stage produces axis-level scores with detailed findings. Findings carry
//! severity, file locations, and auto-fix suggestions. A `RedesignPlan` groups
//! findings into priority batches for incremental improvement.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

/// Context passed to every scoring stage. Contains the project files,
/// optional screenshots, and metadata about the application.
#[derive(Debug, Clone, Serialize)]
pub struct TasteContext {
    /// All relevant project files (CSS, TSX, JSX, HTML, etc.).
    pub files: Vec<ProjectFile>,
    /// Optional screenshots for visual analysis.
    pub screenshots: Option<Vec<Screenshot>>,
    /// Application type (e.g. "saas", "ecommerce", "dashboard").
    pub app_type: String,
    /// Domain of the application (e.g. "fintech", "healthcare").
    pub domain: String,
    /// User style preference override (e.g. "minimal", "bold").
    pub style_preference: Option<String>,
}

/// A single file in the project being scored.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectFile {
    /// Relative path from project root.
    pub path: String,
    /// Full file content.
    pub content: String,
    /// File extension (e.g. "tsx", "css").
    pub extension: String,
}

/// A screenshot of a rendered page for visual analysis.
#[derive(Debug, Clone, Serialize)]
pub struct Screenshot {
    /// URL or local path to the screenshot image.
    pub url: String,
    /// Page identifier (e.g. "/", "/dashboard", "/settings").
    pub page: String,
    /// Viewport size (e.g. "1920x1080", "375x812").
    pub viewport: String,
}

/// Score produced by a single pipeline stage, broken down by axis.
#[derive(Debug, Clone, Serialize)]
pub struct StageScore {
    /// Name of the stage that produced this score.
    pub stage: String,
    /// Per-axis scores within this stage.
    pub axes: HashMap<String, AxisScore>,
    /// Weighted total score for this stage (0.0–100.0).
    pub total: f32,
}

/// Score for a single axis within a stage.
#[derive(Debug, Clone, Serialize)]
pub struct AxisScore {
    /// Raw score for this axis (0.0–100.0).
    pub score: f32,
    /// Weight of this axis in the stage total (0.0–1.0).
    pub weight: f32,
    /// Individual findings that contributed to this score.
    pub findings: Vec<Finding>,
}

/// A specific quality finding — something good or bad detected in the code.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// How severe this finding is.
    pub severity: Severity,
    /// Human-readable description of the finding.
    pub description: String,
    /// File where the finding was detected.
    pub file: Option<String>,
    /// Line number within the file.
    pub line: Option<usize>,
    /// Whether this finding can be automatically fixed.
    pub auto_fixable: bool,
    /// Suggested fix (code or description).
    pub fix_suggestion: Option<String>,
}

/// Severity levels for findings, ordered from most to least critical.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Blocks ship — must fix before release.
    Critical,
    /// Significant quality issue.
    Major,
    /// Small quality issue.
    Minor,
    /// Informational / nice-to-have.
    Info,
}

// ---------------------------------------------------------------------------
// Taste Stage trait
// ---------------------------------------------------------------------------

/// A scoring stage in the taste pipeline. Each stage independently evaluates
/// the project and produces a `StageScore`.
pub trait TasteStage: Send + Sync {
    /// Human-readable name of this stage.
    fn name(&self) -> &str;
    /// Score the given context, returning axis scores and findings.
    fn score(&self, context: &TasteContext) -> Result<StageScore, String>;
}

// ---------------------------------------------------------------------------
// TasteReport
// ---------------------------------------------------------------------------

/// Complete report produced by running the full taste pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct TasteReport {
    /// Overall weighted score across all stages (0.0–100.0).
    pub overall_score: f32,
    /// Per-stage scores.
    pub stages: Vec<StageScore>,
    /// All findings across all stages, sorted by severity.
    pub findings: Vec<Finding>,
    /// Optional redesign plan when score is below threshold.
    pub redesign_plan: Option<RedesignPlan>,
}

// ---------------------------------------------------------------------------
// TastePipeline
// ---------------------------------------------------------------------------

/// The main scoring pipeline. Holds an ordered list of stages and runs them
/// sequentially, aggregating results into a `TasteReport`.
pub struct TastePipeline {
    stages: Vec<Box<dyn TasteStage>>,
}

impl TastePipeline {
    /// Create a new pipeline with custom stages.
    pub fn new(stages: Vec<Box<dyn TasteStage>>) -> Self {
        Self { stages }
    }

    /// Create the default pipeline with all four built-in stages.
    pub fn default_pipeline() -> Self {
        Self {
            stages: vec![
                Box::new(StaticAnalysisStage),
                Box::new(ContentQualityStage),
                Box::new(CodeQualityStage),
                Box::new(VisualAnalysisStage),
            ],
        }
    }

    /// Run all stages and produce a combined `TasteReport`.
    pub fn score(&self, context: &TasteContext) -> TasteReport {
        let mut stage_scores = Vec::new();
        let mut all_findings = Vec::new();

        for stage in &self.stages {
            match stage.score(context) {
                Ok(ss) => {
                    for axis in ss.axes.values() {
                        all_findings.extend(axis.findings.clone());
                    }
                    stage_scores.push(ss);
                }
                Err(e) => {
                    tracing::warn!(stage = stage.name(), error = %e, "Stage failed, skipping");
                }
            }
        }

        // Sort findings by severity (Critical first).
        all_findings.sort_by(|a, b| a.severity.cmp(&b.severity));

        // Overall score: average of stage totals (only stages that ran).
        let overall_score = if stage_scores.is_empty() {
            0.0
        } else {
            stage_scores.iter().map(|s| s.total).sum::<f32>() / stage_scores.len() as f32
        };

        TasteReport {
            overall_score,
            stages: stage_scores,
            findings: all_findings,
            redesign_plan: None,
        }
    }

    /// Run all stages and produce a report. If below `threshold`, attach a redesign plan.
    pub fn score_with_plan(&self, context: &TasteContext, threshold: f32) -> TasteReport {
        let mut report = self.score(context);
        report.redesign_plan = plan_redesign(&report, threshold);
        report
    }
}

// ===========================================================================
// Stage 1: Static Analysis
// ===========================================================================

/// Analyzes CSS/TSX files for visual design quality: color contrast patterns,
/// spacing consistency, typography hierarchy, component structure, responsive
/// breakpoints, and accessibility attributes.
pub struct StaticAnalysisStage;

impl TasteStage for StaticAnalysisStage {
    fn name(&self) -> &str {
        "static_analysis"
    }

    fn score(&self, context: &TasteContext) -> Result<StageScore, String> {
        let mut axes = HashMap::new();

        axes.insert(
            "color_contrast".to_string(),
            score_color_contrast(&context.files),
        );
        axes.insert(
            "spacing_consistency".to_string(),
            score_spacing_consistency(&context.files),
        );
        axes.insert(
            "typography_hierarchy".to_string(),
            score_typography_hierarchy(&context.files),
        );
        axes.insert(
            "component_structure".to_string(),
            score_component_structure(&context.files),
        );
        axes.insert(
            "responsive_breakpoints".to_string(),
            score_responsive_breakpoints(&context.files),
        );
        axes.insert(
            "accessibility_attributes".to_string(),
            score_accessibility_attributes(&context.files),
        );

        let total = compute_stage_total(&axes);
        Ok(StageScore {
            stage: "static_analysis".to_string(),
            axes,
            total,
        })
    }
}

// --- Static analysis axis scorers ---

/// Score color contrast: checks for CSS custom properties for colors, contrast-safe
/// patterns, dark/light mode tokens, and avoids hardcoded hex without variables.
fn score_color_contrast(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 5.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");
    let css_files: Vec<&ProjectFile> = files.iter().filter(|f| f.extension == "css").collect();

    // Check 1: CSS custom properties for colors (--primary, --foreground, etc.)
    let has_color_tokens = css_files.iter().any(|f| {
        f.content.contains("--primary") || f.content.contains("--foreground") || f.content.contains("--background")
            || f.content.contains("--muted") || f.content.contains("--accent")
    });
    if has_color_tokens {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No CSS color custom properties found (--primary, --foreground, etc.)".into(),
            file: css_files.first().map(|f| f.path.clone()),
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add a design token system: :root { --primary: hsl(220, 80%, 50%); --foreground: hsl(0, 0%, 10%); }".into()),
        });
    }

    // Check 2: Using hsl() or oklch() for color values (better for contrast manipulation)
    let has_modern_color = all_content.contains("hsl(") || all_content.contains("oklch(");
    if has_modern_color {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No hsl() or oklch() color functions found — harder to maintain contrast ratios".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use hsl() or oklch() for color values to make contrast adjustments easier".into()),
        });
    }

    // Check 3: dark mode support
    let has_dark_mode = all_content.contains("dark:") || all_content.contains(".dark ") || all_content.contains("prefers-color-scheme");
    if has_dark_mode {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No dark mode support detected".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Add dark mode via Tailwind dark: prefix or prefers-color-scheme media query".into()),
        });
    }

    // Check 4: Avoid raw hex colors AND hardcoded direct Tailwind colors in component files
    let direct_color_violations: Vec<&str> = files.iter()
        .filter(|f| f.extension == "tsx" || f.extension == "jsx")
        .flat_map(|f| {
            let content = &f.content;
            // Hardcoded utility classes that bypass the design token system
            let forbidden = ["text-white", "bg-white", "text-black", "bg-black",
                "bg-orange-", "bg-blue-", "bg-red-", "bg-green-", "bg-yellow-",
                "text-gray-", "bg-gray-", "bg-white/5", "bg-white/10", "border-white/"];
            forbidden.iter()
                .filter(|&&pat| content.contains(pat))
                .copied()
                .collect::<Vec<_>>()
        })
        .collect();

    let hex_in_components = files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx").any(|f| {
        f.content.contains("#fff") || f.content.contains("#000") || contains_hex_color(&f.content)
    });

    if direct_color_violations.is_empty() && !hex_in_components {
        points += 1.0;
    } else {
        let examples = direct_color_violations.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        findings.push(Finding {
            severity: Severity::Major,
            description: format!("Hardcoded color classes bypass design tokens: {}. Use text-foreground, bg-primary, bg-card etc.", if examples.is_empty() { "hex value" } else { &examples }),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Replace text-white/bg-white with text-foreground/bg-background. Replace bg-orange-500 with bg-primary. Use CSS variable tokens throughout.".into()),
        });
    }

    // Check 5: Consistent text color classes
    let has_text_color_system = all_content.contains("text-foreground") || all_content.contains("text-muted")
        || all_content.contains("text-primary") || all_content.contains("text-secondary");
    if has_text_color_system {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No semantic text color classes (text-foreground, text-muted, etc.)".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use semantic color classes like text-foreground, text-muted-foreground for consistency".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Score spacing consistency: checks for a spacing system (design tokens or
/// Tailwind spacing scale), consistent gap/padding usage, and avoids arbitrary values.
fn score_spacing_consistency(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 5.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: Tailwind spacing utilities used
    let has_spacing_system = all_content.contains("gap-") || all_content.contains("space-x-") || all_content.contains("space-y-");
    if has_spacing_system {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No spacing system detected (gap-*, space-x-*, space-y-*)".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use Tailwind spacing utilities (gap-4, space-y-6) for consistent spacing".into()),
        });
    }

    // Check 2: Padding consistency
    let has_padding = all_content.contains("px-") || all_content.contains("py-") || all_content.contains("p-");
    if has_padding {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No padding utilities found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use consistent padding (p-4, px-6, py-8) across components".into()),
        });
    }

    // Check 3: CSS spacing variables
    let has_spacing_vars = all_content.contains("--spacing") || all_content.contains("--gap") || all_content.contains("--padding");
    if has_spacing_vars {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "No CSS custom properties for spacing".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Consider --spacing-sm, --spacing-md, --spacing-lg custom properties".into()),
        });
    }

    // Check 4: Avoid arbitrary spacing values (e.g. p-[13px])
    let has_arbitrary = files.iter().any(|f| {
        f.content.contains("p-[") || f.content.contains("m-[") || f.content.contains("gap-[")
    });
    if !has_arbitrary {
        points += 1.0;
    } else {
        for file in files {
            if file.content.contains("p-[") || file.content.contains("m-[") || file.content.contains("gap-[") {
                findings.push(Finding {
                    severity: Severity::Minor,
                    description: "Arbitrary spacing values break the spacing scale".into(),
                    file: Some(file.path.clone()),
                    line: find_line_containing(&file.content, &["p-[", "m-[", "gap-["]),
                    auto_fixable: true,
                    fix_suggestion: Some("Replace arbitrary values (p-[13px]) with scale values (p-3, p-4)".into()),
                });
                break; // One finding is enough
            }
        }
    }

    // Check 5: Container/max-width constraints
    let has_containers = all_content.contains("max-w-") || all_content.contains("container") || all_content.contains("mx-auto");
    if has_containers {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No container/max-width constraints — content may stretch on wide screens".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add max-w-7xl mx-auto or container class to main content areas".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Score typography hierarchy: checks for heading scale, font-weight variation,
/// line-height settings, and consistent text sizing.
fn score_typography_hierarchy(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 5.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: Heading tags or heading-like classes
    let has_heading_hierarchy = all_content.contains("<h1") && all_content.contains("<h2");
    if has_heading_hierarchy {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "Missing heading hierarchy (need both h1 and h2 elements)".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use semantic heading tags (h1, h2, h3) for proper document hierarchy".into()),
        });
    }

    // Check 2: Font size scale
    let size_classes = ["text-xs", "text-sm", "text-base", "text-lg", "text-xl", "text-2xl", "text-3xl"];
    let sizes_used = size_classes.iter().filter(|c| all_content.contains(**c)).count();
    if sizes_used >= 3 {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: format!("Only {} distinct text size classes used — need at least 3 for hierarchy", sizes_used),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use a range of text sizes (text-sm, text-base, text-lg, text-2xl) for visual hierarchy".into()),
        });
    }

    // Check 3: Font weight variation
    let has_weight_variation = all_content.contains("font-bold") || all_content.contains("font-semibold");
    let has_normal_weight = all_content.contains("font-normal") || all_content.contains("font-medium") || !all_content.contains("font-bold");
    if has_weight_variation && has_normal_weight {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "Limited font weight variation".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use font-medium for body, font-semibold for subheadings, font-bold for headings".into()),
        });
    }

    // Check 4: Line height settings
    let has_leading = all_content.contains("leading-") || all_content.contains("line-height");
    if has_leading {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No explicit line-height/leading classes found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add leading-relaxed or leading-normal for better readability".into()),
        });
    }

    // Check 5: Font family system
    let has_font_system = all_content.contains("font-sans") || all_content.contains("font-mono") || all_content.contains("--font-");
    if has_font_system {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "No explicit font family system".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Define font-sans or custom --font-heading/--font-body variables".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Score component structure: checks for proper React component patterns,
/// composition, and separation of concerns.
fn score_component_structure(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 5.0;

    let tsx_files: Vec<&ProjectFile> = files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx").collect();

    if tsx_files.is_empty() {
        return AxisScore {
            score: 50.0, // neutral when no TSX files
            weight,
            findings: vec![Finding {
                severity: Severity::Info,
                description: "No TSX/JSX files found to analyze".into(),
                file: None,
                line: None,
                auto_fixable: false,
                fix_suggestion: None,
            }],
        };
    }

    // Check 1: Components use export default or named exports
    let has_exports = tsx_files.iter().any(|f| f.content.contains("export default") || f.content.contains("export function") || f.content.contains("export const"));
    if has_exports {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No exported components found".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Export components with 'export default function ComponentName'".into()),
        });
    }

    // Check 2: Components directory or modular file structure
    let has_components_dir = files.iter().any(|f| f.path.contains("components/") || f.path.contains("components\\"));
    if has_components_dir {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No components/ directory — all UI code may be in page files".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Extract reusable UI into a components/ directory".into()),
        });
    }

    // Check 3: Reasonable component sizes (< 200 lines)
    let oversized: Vec<&ProjectFile> = tsx_files.iter().filter(|f| f.content.lines().count() > 200).copied().collect();
    if oversized.is_empty() {
        points += 1.0;
    } else {
        for f in &oversized {
            findings.push(Finding {
                severity: Severity::Minor,
                description: format!("Component file exceeds 200 lines ({} lines)", f.content.lines().count()),
                file: Some(f.path.clone()),
                line: None,
                auto_fixable: false,
                fix_suggestion: Some("Split large components into smaller, focused sub-components".into()),
            });
        }
    }

    // Check 4: Uses composition patterns (children, slots)
    let has_composition = tsx_files.iter().any(|f| f.content.contains("children") || f.content.contains("props.children") || f.content.contains("{children}"));
    if has_composition {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "No component composition patterns (children prop) detected".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use children prop for flexible component composition".into()),
        });
    }

    // Check 5: Uses React hooks (useState, useEffect, etc.)
    let has_hooks = tsx_files.iter().any(|f| f.content.contains("useState") || f.content.contains("useEffect") || f.content.contains("useMemo"));
    if has_hooks {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "No React hooks found — components may be purely static".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: None,
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Score responsive breakpoints: checks for mobile-first design, breakpoint usage,
/// responsive utilities, and viewport meta tag.
fn score_responsive_breakpoints(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 5.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: Tailwind responsive prefixes
    let breakpoint_prefixes = ["sm:", "md:", "lg:", "xl:", "2xl:"];
    let bp_count = breakpoint_prefixes.iter().filter(|p| all_content.contains(**p)).count();
    if bp_count >= 2 {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: format!("Only {} responsive breakpoint prefixes used — need at least 2", bp_count),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use sm: and md: prefixes minimum for mobile-first responsive design".into()),
        });
    }

    // Check 2: Flex/grid responsive layouts
    let has_responsive_layout = all_content.contains("md:flex") || all_content.contains("md:grid")
        || all_content.contains("lg:flex") || all_content.contains("sm:flex");
    if has_responsive_layout {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No responsive layout shifts (md:flex, md:grid, etc.)".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use flex-col md:flex-row pattern for responsive layouts".into()),
        });
    }

    // Check 3: Mobile-first column stacking
    let has_col_stacking = all_content.contains("flex-col") && (all_content.contains("md:flex-row") || all_content.contains("lg:flex-row"));
    if has_col_stacking {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No mobile-first column stacking pattern (flex-col → md:flex-row)".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use 'flex flex-col md:flex-row' for responsive stacking".into()),
        });
    }

    // Check 4: Responsive visibility
    let has_responsive_visibility = all_content.contains("hidden md:") || all_content.contains("md:hidden")
        || all_content.contains("lg:hidden") || all_content.contains("hidden sm:");
    if has_responsive_visibility {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No responsive visibility toggles (hidden md:block, etc.)".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use 'hidden md:block' to show/hide elements at breakpoints".into()),
        });
    }

    // Check 5: Viewport meta tag
    let has_viewport = all_content.contains("viewport") && all_content.contains("width=device-width");
    if has_viewport {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Critical,
            description: "Missing viewport meta tag — page will not render properly on mobile".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Score accessibility attributes: checks for ARIA labels, alt text, form labels,
/// focus indicators, keyboard navigation support, and screen reader content.
fn score_accessibility_attributes(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 6.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: ARIA labels
    let has_aria = all_content.contains("aria-label") || all_content.contains("aria-describedby") || all_content.contains("aria-labelledby");
    if has_aria {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No ARIA labels found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add aria-label to interactive elements (buttons, inputs, links without visible text)".into()),
        });
    }

    // Check 2: Image alt text
    let has_img = all_content.contains("<img") || all_content.contains("<Image");
    let has_alt = all_content.contains("alt=") || all_content.contains("alt =");
    if !has_img || has_alt {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Critical,
            description: "Images found without alt attributes".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add descriptive alt text to all <img> and <Image> elements".into()),
        });
    }

    // Check 3: Form labels
    let has_form_labels = all_content.contains("<label") || all_content.contains("htmlFor");
    let has_inputs = all_content.contains("<input") || all_content.contains("<Input") || all_content.contains("<textarea");
    if !has_inputs || has_form_labels {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "Form inputs found without associated <label> elements".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add <label htmlFor=\"fieldId\"> for every form input".into()),
        });
    }

    // Check 4: Focus indicators
    let has_focus = all_content.contains("focus-visible:") || all_content.contains("focus:ring") || all_content.contains("focus:outline");
    if has_focus {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No focus indicators for keyboard navigation".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add focus-visible:ring-2 focus-visible:ring-offset-2 to interactive elements".into()),
        });
    }

    // Check 5: ARIA roles
    let has_roles = all_content.contains("role=");
    if has_roles {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No explicit ARIA roles found".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Add role attributes to landmark regions (role=\"main\", role=\"navigation\")".into()),
        });
    }

    // Check 6: Screen reader only content
    let has_sr_only = all_content.contains("sr-only") || all_content.contains("visually-hidden");
    if has_sr_only {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "No screen-reader-only content (sr-only class)".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add sr-only text for icon buttons and visual-only content".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

// ===========================================================================
// Stage 2: Content Quality
// ===========================================================================

/// Checks for placeholder text, realistic data, and presence of important
/// UI states (error, empty, loading).
pub struct ContentQualityStage;

impl TasteStage for ContentQualityStage {
    fn name(&self) -> &str {
        "content_quality"
    }

    fn score(&self, context: &TasteContext) -> Result<StageScore, String> {
        let mut axes = HashMap::new();

        axes.insert(
            "no_lorem_ipsum".to_string(),
            score_no_lorem_ipsum(&context.files),
        );
        axes.insert(
            "realistic_data".to_string(),
            score_realistic_data(&context.files),
        );
        axes.insert(
            "error_states".to_string(),
            score_error_states(&context.files),
        );
        axes.insert(
            "empty_states".to_string(),
            score_empty_states(&context.files),
        );
        axes.insert(
            "loading_states".to_string(),
            score_loading_states(&context.files),
        );

        let total = compute_stage_total(&axes);
        Ok(StageScore {
            stage: "content_quality".to_string(),
            axes,
            total,
        })
    }
}

// --- Content quality axis scorers ---

/// Detect and penalize lorem ipsum and other placeholder text patterns.
fn score_no_lorem_ipsum(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.25;
    let mut findings = Vec::new();

    let placeholder_patterns = [
        "lorem ipsum",
        "Lorem ipsum",
        "LOREM IPSUM",
        "dolor sit amet",
        "consectetur adipiscing",
        "placeholder text",
        "TODO:",
        "FIXME:",
        "XXX:",
        "sample text",
        "example text",
        "your text here",
        "Enter your",
        "Type something",
        "foo bar",
        "John Doe",
        "Jane Doe",
        "Coming soon",
        "Under construction",
        "This is a description",
        "Click here for",
        "Insert text",
        "User 1",
        "User 2",
        "Item 1",
        "Item 2",
        "Product 1",
        "Test User",
    ];

    let mut violations = 0;

    for file in files {
        let lower = file.content.to_lowercase();
        for pattern in &placeholder_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                violations += 1;
                findings.push(Finding {
                    severity: if pattern.to_lowercase().contains("lorem") {
                        Severity::Critical
                    } else {
                        Severity::Minor
                    },
                    description: format!("Placeholder text found: '{}'", pattern),
                    file: Some(file.path.clone()),
                    line: find_line_containing(&file.content, &[pattern]),
                    auto_fixable: true,
                    fix_suggestion: Some(format!("Replace '{}' with realistic, domain-appropriate content", pattern)),
                });
                break; // One finding per file per pattern type
            }
        }
    }

    // Score: 100 if no violations, decreasing with each one
    let score = (100.0 - (violations as f32 * 15.0)).max(0.0);

    AxisScore {
        score,
        weight,
        findings,
    }
}

/// Check that data shown in the UI looks realistic (not "Item 1", "User 123", etc.).
fn score_realistic_data(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();

    let generic_data_patterns = [
        "Item 1",
        "Item 2",
        "Item 3",
        "User 1",
        "User 2",
        "Product 1",
        "Product 2",
        "Test data",
        "test@test.com",
        "test@example.com",
        "123-456-7890",
        "000-000-0000",
        "$0.00",
        "$99.99",
        "Lorem",
        "Ipsum",
        "example.com",
    ];

    let mut violations = 0;

    for file in files {
        for pattern in &generic_data_patterns {
            if file.content.contains(pattern) {
                violations += 1;
                findings.push(Finding {
                    severity: Severity::Minor,
                    description: format!("Generic/test data found: '{}'", pattern),
                    file: Some(file.path.clone()),
                    line: find_line_containing(&file.content, &[pattern]),
                    auto_fixable: true,
                    fix_suggestion: Some(format!("Replace '{}' with realistic domain-appropriate data", pattern)),
                });
                break;
            }
        }
    }

    let score = (100.0 - (violations as f32 * 12.0)).max(0.0);

    AxisScore {
        score,
        weight,
        findings,
    }
}

/// Check that the app has proper error state handling in the UI.
fn score_error_states(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 4.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: Error boundary or error component
    let has_error_boundary = all_content.contains("ErrorBoundary") || all_content.contains("error.tsx") || all_content.contains("error.jsx");
    if has_error_boundary {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No ErrorBoundary or error page component found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add an error.tsx file or ErrorBoundary component for graceful error handling".into()),
        });
    }

    // Check 2: Error state in data fetching
    let has_error_handling = all_content.contains("isError") || all_content.contains("error &&")
        || all_content.contains("catch(") || all_content.contains(".error");
    if has_error_handling {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No error state handling in data fetching".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Add error state rendering: if (error) return <ErrorMessage />".into()),
        });
    }

    // Check 3: User-friendly error messages
    let has_friendly_errors = all_content.contains("Something went wrong") || all_content.contains("try again")
        || all_content.contains("error message") || all_content.contains("oops");
    if has_friendly_errors {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No user-friendly error messages found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add human-readable error messages like 'Something went wrong. Please try again.'".into()),
        });
    }

    // Check 4: 404 / Not Found page
    let has_not_found = all_content.contains("not-found") || all_content.contains("NotFound") || all_content.contains("404");
    if has_not_found {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No 404/Not Found page".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add a not-found.tsx page for better UX on missing routes".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Check that the app handles empty states gracefully.
fn score_empty_states(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 3.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: Empty state component or pattern
    let has_empty_state = all_content.contains("EmptyState") || all_content.contains("empty-state")
        || all_content.contains("no results") || all_content.contains("No results")
        || all_content.contains("nothing here") || all_content.contains("No data");
    if has_empty_state {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No empty state UI pattern found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add empty state messaging when lists/tables have no data".into()),
        });
    }

    // Check 2: Conditional rendering for empty arrays
    let has_length_check = all_content.contains(".length === 0") || all_content.contains(".length == 0")
        || all_content.contains(".length > 0") || all_content.contains(".length ?");
    if has_length_check {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No array length checks for conditional empty state rendering".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Check array.length before mapping and show empty state when 0".into()),
        });
    }

    // Check 3: CTA in empty state (guide user)
    let has_empty_cta = all_content.contains("Get started") || all_content.contains("Create your first")
        || all_content.contains("Add your first") || all_content.contains("Start by");
    if has_empty_cta {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "Empty states don't include a call-to-action to guide the user".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add a CTA button in empty states like 'Create your first item'".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Check that the app has loading states and feedback.
fn score_loading_states(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 4.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: Loading indicator component
    let has_loading = all_content.contains("Spinner") || all_content.contains("spinner")
        || all_content.contains("Loading") || all_content.contains("loading");
    if has_loading {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No loading indicator found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Add a loading spinner or indicator for async operations".into()),
        });
    }

    // Check 2: Skeleton loaders
    let has_skeleton = all_content.contains("Skeleton") || all_content.contains("skeleton")
        || all_content.contains("shimmer") || all_content.contains("placeholder");
    if has_skeleton {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No skeleton loaders for content placeholders".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use skeleton components during data loading for better perceived performance".into()),
        });
    }

    // Check 3: isLoading / isPending state checks
    let has_loading_state = all_content.contains("isLoading") || all_content.contains("isPending")
        || all_content.contains("loading &&") || all_content.contains("loading ?");
    if has_loading_state {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No conditional loading state rendering".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Add loading state checks: if (isLoading) return <Skeleton />".into()),
        });
    }

    // Check 4: Suspense boundaries
    let has_suspense = all_content.contains("Suspense") || all_content.contains("loading.tsx");
    if has_suspense {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "No React Suspense boundaries found".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use <Suspense fallback={<Loading />}> for code-split components".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

// ===========================================================================
// Stage 3: Code Quality
// ===========================================================================

/// Checks code patterns: inline styles, naming consistency, component size,
/// prop typing, and hardcoded values.
pub struct CodeQualityStage;

impl TasteStage for CodeQualityStage {
    fn name(&self) -> &str {
        "code_quality"
    }

    fn score(&self, context: &TasteContext) -> Result<StageScore, String> {
        let mut axes = HashMap::new();

        axes.insert(
            "no_inline_styles".to_string(),
            score_no_inline_styles(&context.files),
        );
        axes.insert(
            "naming_consistency".to_string(),
            score_naming_consistency(&context.files),
        );
        axes.insert(
            "component_size".to_string(),
            score_component_size(&context.files),
        );
        axes.insert(
            "prop_typing".to_string(),
            score_prop_typing(&context.files),
        );
        axes.insert(
            "no_hardcoded_values".to_string(),
            score_no_hardcoded_values(&context.files),
        );
        axes.insert(
            "modern_stack_adoption".to_string(),
            score_modern_stack_adoption(&context.files),
        );
        axes.insert(
            "no_broken_hrefs".to_string(),
            score_no_broken_hrefs(&context.files),
        );
        axes.insert(
            "no_alert_usage".to_string(),
            score_no_alert_usage(&context.files),
        );
        axes.insert(
            "form_label_association".to_string(),
            score_form_label_association(&context.files),
        );

        let total = compute_stage_total(&axes);
        Ok(StageScore {
            stage: "code_quality".to_string(),
            axes,
            total,
        })
    }
}

// --- Code quality axis scorers ---

/// Penalize inline style attributes — prefer Tailwind classes or CSS modules.
fn score_no_inline_styles(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.25;
    let mut findings = Vec::new();
    let mut violations = 0;

    for file in files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx") {
        let mut line_num = 0;
        for line in file.content.lines() {
            line_num += 1;
            // Match style={{ ... }} pattern (inline React styles)
            if line.contains("style={{") || line.contains("style={") {
                violations += 1;
                findings.push(Finding {
                    severity: Severity::Minor,
                    description: "Inline style attribute found — use Tailwind classes or CSS modules".into(),
                    file: Some(file.path.clone()),
                    line: Some(line_num),
                    auto_fixable: true,
                    fix_suggestion: Some("Replace style={{...}} with equivalent Tailwind utility classes".into()),
                });
            }
        }
    }

    // Cap findings to avoid noise
    findings.truncate(10);

    let score = (100.0 - (violations as f32 * 10.0)).max(0.0);

    AxisScore {
        score,
        weight,
        findings,
    }
}

/// Check naming consistency: PascalCase components, camelCase variables,
/// kebab-case files.
fn score_naming_consistency(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 3.0;

    let tsx_files: Vec<&ProjectFile> = files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx").collect();

    // Check 1: Component files use kebab-case or PascalCase names (not mixed)
    let file_names: Vec<&str> = tsx_files.iter().filter_map(|f| {
        f.path.rsplit('/').next().or_else(|| f.path.rsplit('\\').next())
    }).collect();

    let kebab_count = file_names.iter().filter(|n| n.contains('-')).count();
    let pascal_count = file_names.iter().filter(|n| {
        n.chars().next().is_some_and(|c| c.is_uppercase())
    }).count();

    // Consistent if all one style (or very few files)
    if file_names.len() <= 2 || kebab_count == 0 || pascal_count == 0 {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: format!("Mixed file naming: {} kebab-case, {} PascalCase files", kebab_count, pascal_count),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Pick one naming convention for component files and stick to it".into()),
        });
    }

    // Check 2: Component functions use PascalCase
    let has_pascal_components = tsx_files.iter().any(|f| {
        f.content.contains("function App") || f.content.contains("function Home")
            || f.content.contains("function Page") || f.content.contains("export default function")
            || f.content.contains("export function")
    });
    if has_pascal_components || tsx_files.is_empty() {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "Component functions may not follow PascalCase convention".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Name component functions in PascalCase: function MyComponent()".into()),
        });
    }

    // Check 3: No single-letter variable names in JSX (poor readability)
    let has_single_letter = tsx_files.iter().any(|f| {
        f.content.contains(".map((x)") || f.content.contains(".map((i)")
            || f.content.contains(".map(x =>") || f.content.contains(".map(i =>")
    });
    if !has_single_letter {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "Single-letter variable names in .map() — use descriptive names".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Use descriptive names: items.map((item) => ...) instead of items.map((x) => ...)".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Check that individual component files are not excessively large.
fn score_component_size(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();

    let tsx_files: Vec<&ProjectFile> = files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx").collect();

    if tsx_files.is_empty() {
        return AxisScore {
            score: 100.0,
            weight,
            findings: vec![],
        };
    }

    let mut oversized_count = 0;
    let mut large_count = 0;

    for file in &tsx_files {
        let lines = file.content.lines().count();
        if lines > 300 {
            oversized_count += 1;
            findings.push(Finding {
                severity: Severity::Major,
                description: format!("Component is {} lines — should be under 200", lines),
                file: Some(file.path.clone()),
                line: None,
                auto_fixable: false,
                fix_suggestion: Some("Extract sub-components, custom hooks, or utility functions to reduce file size".into()),
            });
        } else if lines > 150 {
            large_count += 1;
            findings.push(Finding {
                severity: Severity::Minor,
                description: format!("Component is {} lines — consider splitting", lines),
                file: Some(file.path.clone()),
                line: None,
                auto_fixable: false,
                fix_suggestion: Some("Consider extracting reusable parts into separate components".into()),
            });
        }
    }

    let total = tsx_files.len();
    let good = total.saturating_sub(oversized_count + large_count);
    let score = if total == 0 {
        100.0
    } else {
        ((good as f32 / total as f32) * 80.0) + 20.0 // floor at 20
    };

    AxisScore {
        score: score.min(100.0),
        weight,
        findings,
    }
}

/// Check that component props are properly typed (TypeScript interfaces/types).
fn score_prop_typing(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 3.0;

    let tsx_files: Vec<&ProjectFile> = files.iter().filter(|f| f.extension == "tsx").collect();

    if tsx_files.is_empty() {
        return AxisScore {
            score: 80.0, // neutral
            weight,
            findings: vec![],
        };
    }

    // Check 1: Has TypeScript interfaces or types for props
    let has_prop_types = tsx_files.iter().any(|f| {
        f.content.contains("interface ") || f.content.contains("type ") && f.content.contains("Props")
    });
    if has_prop_types {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No TypeScript prop interfaces/types found".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Define prop types: interface MyComponentProps { title: string; }".into()),
        });
    }

    // Check 2: Avoids `any` type
    let has_any = tsx_files.iter().any(|f| {
        f.content.contains(": any") || f.content.contains(":any") || f.content.contains("as any")
    });
    if !has_any {
        points += 1.0;
    } else {
        for file in &tsx_files {
            if file.content.contains(": any") || file.content.contains(":any") || file.content.contains("as any") {
                findings.push(Finding {
                    severity: Severity::Minor,
                    description: "Usage of 'any' type — reduces type safety".into(),
                    file: Some(file.path.clone()),
                    line: find_line_containing(&file.content, &[": any", ":any", "as any"]),
                    auto_fixable: false,
                    fix_suggestion: Some("Replace 'any' with proper types".into()),
                });
                break;
            }
        }
    }

    // Check 3: Props destructuring (not props.x)
    let has_destructured = tsx_files.iter().any(|f| {
        f.content.contains("{ ") && (f.content.contains("}: ") || f.content.contains("} :"))
    });
    let has_props_dot = tsx_files.iter().any(|f| f.content.contains("props."));
    if has_destructured || !has_props_dot {
        points += 1.0;
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            description: "Props accessed via props.x instead of destructuring".into(),
            file: None,
            line: None,
            auto_fixable: true,
            fix_suggestion: Some("Destructure props: function MyComp({ title, desc }: Props) instead of props.title".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Check for hardcoded magic numbers, strings, and URLs in component code.
fn score_no_hardcoded_values(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.20;
    let mut findings = Vec::new();
    let mut violations = 0;

    let hardcoded_patterns: &[(&str, &str)] = &[
        ("http://localhost", "Hardcoded localhost URL"),
        ("https://api.", "Hardcoded API URL"),
        ("127.0.0.1", "Hardcoded IP address"),
        ("sk-", "Possible hardcoded API key"),
        ("pk_test_", "Hardcoded Stripe test key"),
        ("sk_test_", "Hardcoded Stripe test secret"),
    ];

    for file in files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx" || f.extension == "ts") {
        for (pattern, desc) in hardcoded_patterns {
            if file.content.contains(pattern) {
                violations += 1;
                findings.push(Finding {
                    severity: if pattern.starts_with("sk-") || pattern.starts_with("pk_test") || pattern.starts_with("sk_test") {
                        Severity::Critical
                    } else {
                        Severity::Minor
                    },
                    description: format!("{} in component file", desc),
                    file: Some(file.path.clone()),
                    line: find_line_containing(&file.content, &[pattern]),
                    auto_fixable: true,
                    fix_suggestion: Some("Move to environment variable or config file".into()),
                });
                break;
            }
        }
    }

    // Also check for magic numbers in className strings (fine) vs logic (bad)
    for file in files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx") {
        for (line_num, line) in file.content.lines().enumerate() {
            // Skip className attributes, import lines, and array indices
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("import") || trimmed.contains("className") {
                continue;
            }
            // Check for magic numbers in conditionals or assignments
            if (trimmed.contains("> 1000") || trimmed.contains("< 1000") || trimmed.contains("=== 0")
                || trimmed.contains("setTimeout") && trimmed.contains("5000"))
                && !trimmed.contains("length")
            {
                violations += 1;
                findings.push(Finding {
                    severity: Severity::Info,
                    description: "Possible magic number — consider extracting to a named constant".into(),
                    file: Some(file.path.clone()),
                    line: Some(line_num + 1),
                    auto_fixable: true,
                    fix_suggestion: Some("Extract magic numbers to named constants: const MAX_ITEMS = 1000;".into()),
                });
                break;
            }
        }
    }

    findings.truncate(10);
    let score = (100.0 - (violations as f32 * 12.0)).max(0.0);

    AxisScore {
        score,
        weight,
        findings,
    }
}

// --- Modern stack quality axis scorers ---

/// Score adoption of the modern component stack: shadcn/ui, Lucide icons, sonner, next-themes.
fn score_modern_stack_adoption(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut points = 0.0_f32;
    let max_points = 5.0;

    let all_content: String = files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");

    // Check 1: shadcn/ui components in use
    let has_shadcn = all_content.contains("@/components/ui/") || all_content.contains("components/ui/button")
        || all_content.contains("from \"@/components/ui") || all_content.contains("from '@/components/ui");
    if has_shadcn {
        points += 1.5;
    } else {
        findings.push(Finding {
            severity: Severity::Major,
            description: "No shadcn/ui component imports found — UI components appear to be custom or missing".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use shadcn/ui components: import { Button } from \"@/components/ui/button\"".into()),
        });
    }

    // Check 2: Lucide React icons (not emoji, not other icon libs)
    let has_lucide = all_content.contains("lucide-react");
    let has_other_icon_lib = all_content.contains("react-icons") || all_content.contains("@heroicons") || all_content.contains("@fortawesome");
    if has_lucide && !has_other_icon_lib {
        points += 1.0;
    } else if has_other_icon_lib {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "Non-standard icon library in use — prefer lucide-react for consistency".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Replace icon library with lucide-react: import { X, Plus } from \"lucide-react\"".into()),
        });
    }

    // Check 3: sonner for toasts (not alert, not react-toastify)
    let has_sonner = all_content.contains("from \"sonner\"") || all_content.contains("from 'sonner'");
    let has_old_toast = all_content.contains("react-toastify") || all_content.contains("react-hot-toast");
    if has_sonner && !has_old_toast {
        points += 1.0;
    } else if has_old_toast {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "Older toast library in use — prefer sonner for modern notification UX".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Replace with sonner: import { toast } from \"sonner\"".into()),
        });
    }

    // Check 4: Drizzle ORM (not raw sqlite/mysql driver)
    let has_drizzle = all_content.contains("drizzle-orm") || all_content.contains("from \"drizzle");
    let has_raw_driver = (all_content.contains("require('sqlite3')") || all_content.contains("require(\"sqlite3\")"))
        && !has_drizzle;
    if has_drizzle {
        points += 1.0;
    } else if has_raw_driver {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "Raw database driver in use — prefer Drizzle ORM for type-safe queries".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Use Drizzle ORM: import { db } from \"@/db\"".into()),
        });
    }

    // Check 5: Zod for validation
    let has_zod = all_content.contains("from \"zod\"") || all_content.contains("from 'zod'");
    if has_zod {
        points += 0.5;
    } else {
        findings.push(Finding {
            severity: Severity::Minor,
            description: "No Zod schema validation found — forms and API inputs may lack runtime validation".into(),
            file: None,
            line: None,
            auto_fixable: false,
            fix_suggestion: Some("Add Zod validation: import { z } from \"zod\"".into()),
        });
    }

    AxisScore {
        score: (points / max_points) * 100.0,
        weight,
        findings,
    }
}

/// Score href quality — penalize placeholder href="#" which causes page jump and broken navigation.
fn score_no_broken_hrefs(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.10;
    let mut findings = Vec::new();
    let mut violations = 0;

    for file in files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx" || f.extension == "html") {
        let mut line_num = 0;
        for line in file.content.lines() {
            line_num += 1;
            if line.contains("href=\"#\"") || line.contains("href='#'") {
                violations += 1;
                findings.push(Finding {
                    severity: Severity::Major,
                    description: "Broken placeholder href=\"#\" — causes page scroll-to-top and breaks navigation".into(),
                    file: Some(file.path.clone()),
                    line: Some(line_num),
                    auto_fixable: true,
                    fix_suggestion: Some("Replace href=\"#\" with a real route (e.g. \"/feature\") or use a <button> for actions".into()),
                });
                if violations >= 10 { break; }
            }
        }
        if violations >= 10 { break; }
    }

    let score = (100.0 - (violations as f32 * 20.0)).max(0.0);
    AxisScore { score, weight, findings }
}

/// Score for browser alert() usage — penalize native alerts (poor UX) in favor of toast.
fn score_no_alert_usage(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.10;
    let mut findings = Vec::new();
    let mut violations = 0;

    for file in files.iter().filter(|f| matches!(f.extension.as_str(), "tsx" | "jsx" | "ts" | "js")) {
        let mut line_num = 0;
        for line in file.content.lines() {
            line_num += 1;
            let trimmed = line.trim();
            if (trimmed.contains("alert(") || trimmed.contains("window.alert(") || trimmed.contains("window.confirm("))
                && !trimmed.starts_with("//")
            {
                violations += 1;
                findings.push(Finding {
                    severity: Severity::Major,
                    description: "Native alert()/confirm() blocks the UI thread and breaks the app's visual language".into(),
                    file: Some(file.path.clone()),
                    line: Some(line_num),
                    auto_fixable: true,
                    fix_suggestion: Some("Replace with toast.success() / toast.error() from sonner, or a Dialog component for confirmations".into()),
                });
                if violations >= 5 { break; }
            }
        }
        if violations >= 5 { break; }
    }

    let score = (100.0 - (violations as f32 * 25.0)).max(0.0);
    AxisScore { score, weight, findings }
}

/// Score form label associations — every input should have a properly associated label.
fn score_form_label_association(files: &[ProjectFile]) -> AxisScore {
    let weight = 0.15;
    let mut findings = Vec::new();
    let mut inputs_without_label = 0;
    let mut inputs_total = 0;

    for file in files.iter().filter(|f| f.extension == "tsx" || f.extension == "jsx") {
        let mut line_num = 0;
        for line in file.content.lines() {
            line_num += 1;
            let trimmed = line.trim();

            // Count Input components that likely need labels
            if (trimmed.contains("<Input") || trimmed.contains("<input")) && !trimmed.starts_with("//") {
                inputs_total += 1;
                // Check if there's a nearby label in the surrounding context (simplified: check same file)
                // A proper check would parse JSX — this is a heuristic
                let has_label_in_file = file.content.contains("htmlFor=") || file.content.contains("<Label");
                let has_aria = line.contains("aria-label") || line.contains("aria-labelledby");
                if !has_label_in_file && !has_aria {
                    inputs_without_label += 1;
                    findings.push(Finding {
                        severity: Severity::Major,
                        description: "Input element found without apparent label association (no htmlFor/Label or aria-label)".into(),
                        file: Some(file.path.clone()),
                        line: Some(line_num),
                        auto_fixable: true,
                        fix_suggestion: Some("Add <Label htmlFor=\"field-id\"> paired with <Input id=\"field-id\" />".into()),
                    });
                    break; // One finding per file
                }
            }
        }
    }

    let _ = inputs_total;
    let score = if inputs_without_label == 0 { 100.0 } else { (100.0 - inputs_without_label as f32 * 20.0).max(0.0) };
    findings.truncate(10);
    AxisScore { score, weight, findings }
}

// ===========================================================================
// Stage 4: Visual Analysis (stub — requires multimodal LLM)
// ===========================================================================

/// Optional visual analysis stage. When screenshots are provided, this would
/// call a multimodal LLM to evaluate visual quality. Currently returns a
/// default neutral score when no screenshots are available.
pub struct VisualAnalysisStage;

impl TasteStage for VisualAnalysisStage {
    fn name(&self) -> &str {
        "visual_analysis"
    }

    fn score(&self, context: &TasteContext) -> Result<StageScore, String> {
        let screenshots = context.screenshots.as_ref();

        if screenshots.is_none() || screenshots.is_none_or(|s| s.is_empty()) {
            // No screenshots — return neutral defaults.
            let mut axes = HashMap::new();
            axes.insert(
                "visual_appeal".to_string(),
                AxisScore {
                    score: 70.0,
                    weight: 0.30,
                    findings: vec![Finding {
                        severity: Severity::Info,
                        description: "No screenshots provided — visual analysis skipped".into(),
                        file: None,
                        line: None,
                        auto_fixable: false,
                        fix_suggestion: Some("Provide screenshots for full visual quality analysis".into()),
                    }],
                },
            );
            axes.insert(
                "layout_balance".to_string(),
                AxisScore {
                    score: 70.0,
                    weight: 0.25,
                    findings: vec![],
                },
            );
            axes.insert(
                "whitespace_usage".to_string(),
                AxisScore {
                    score: 70.0,
                    weight: 0.25,
                    findings: vec![],
                },
            );
            axes.insert(
                "visual_consistency".to_string(),
                AxisScore {
                    score: 70.0,
                    weight: 0.20,
                    findings: vec![],
                },
            );

            let total = compute_stage_total(&axes);
            return Ok(StageScore {
                stage: "visual_analysis".to_string(),
                axes,
                total,
            });
        }

        // Code-based visual quality heuristics (works without screenshots).
        // Analyzes CSS/Tailwind patterns to infer visual quality.
        let all_content = context.files.iter()
            .filter(|f| f.path.ends_with(".tsx") || f.path.ends_with(".css"))
            .map(|f| f.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // Visual appeal: check for design system usage, gradients, shadows, animations
        let mut appeal_score: f32 = 50.0;
        let mut appeal_findings = Vec::new();
        let appeal_signals: [(&str, f32); 12] = [
            ("shadow", 5.0), ("gradient", 5.0), ("rounded", 3.0),
            ("backdrop-blur", 4.0), ("animate-", 4.0), ("transition", 3.0),
            ("framer-motion", 6.0), ("motion.", 5.0), ("opacity", 2.0),
            ("hover:", 3.0), ("dark:", 3.0), ("ring-", 2.0),
        ];
        for (signal, boost) in &appeal_signals {
            if all_content.contains(signal) { appeal_score += boost; }
        }
        appeal_score = appeal_score.min(95.0);

        // Layout balance: check for grid/flex, max-width containers, responsive
        let mut layout_score: f32 = 50.0;
        let layout_signals: [(&str, f32); 9] = [
            ("flex", 5.0), ("grid", 5.0), ("max-w-", 5.0),
            ("mx-auto", 4.0), ("gap-", 3.0), ("space-", 3.0),
            ("justify-", 3.0), ("items-", 3.0), ("container", 3.0),
        ];
        for (signal, boost) in &layout_signals {
            if all_content.contains(signal) { layout_score += boost; }
        }
        layout_score = layout_score.min(95.0);
        if !all_content.contains("max-w-") && !all_content.contains("container") {
            appeal_findings.push(Finding {
                severity: Severity::Minor,
                description: "No max-width container found — content may stretch too wide on large screens".into(),
                file: None, line: None, auto_fixable: false,
                fix_suggestion: Some("Wrap content in a max-w-7xl mx-auto container".into()),
            });
        }

        // Whitespace: check for consistent spacing scale usage
        let mut whitespace_score: f32 = 55.0;
        let spacing_tokens = ["p-", "px-", "py-", "m-", "mx-", "my-", "gap-", "space-"];
        let spacing_count: usize = spacing_tokens.iter()
            .map(|t| all_content.matches(t).count())
            .sum();
        whitespace_score += (spacing_count as f32 * 0.3).min(30.0);
        whitespace_score = whitespace_score.min(95.0);

        // Visual consistency: check for design tokens, CSS variables, consistent colors
        let mut consistency_score: f32 = 55.0;
        let consistency_signals: [(&str, f32); 9] = [
            ("--", 5.0), ("hsl(", 4.0), ("var(--", 5.0),
            ("theme", 3.0), ("cn(", 3.0), ("cva(", 3.0),
            ("@apply", 2.0), ("text-slate-", 2.0), ("text-gray-", 2.0),
        ];
        for (signal, boost) in &consistency_signals {
            if all_content.contains(signal) { consistency_score += boost; }
        }
        consistency_score = consistency_score.min(95.0);

        let mut axes = HashMap::new();
        axes.insert("visual_appeal".to_string(), AxisScore {
            score: appeal_score, weight: 0.30, findings: appeal_findings,
        });
        axes.insert("layout_balance".to_string(), AxisScore {
            score: layout_score, weight: 0.25, findings: vec![],
        });
        axes.insert("whitespace_usage".to_string(), AxisScore {
            score: whitespace_score, weight: 0.25, findings: vec![],
        });
        axes.insert("visual_consistency".to_string(), AxisScore {
            score: consistency_score, weight: 0.20, findings: vec![],
        });

        let total = compute_stage_total(&axes);
        Ok(StageScore {
            stage: "visual_analysis".to_string(),
            axes,
            total,
        })
    }
}

// ===========================================================================
// Redesign Plan
// ===========================================================================

/// A plan to incrementally improve a project's taste score, organized into
/// priority batches.
#[derive(Debug, Clone, Serialize)]
pub struct RedesignPlan {
    /// Ordered batches of findings to fix, highest priority first.
    pub batches: Vec<RedesignBatch>,
    /// Estimated score after all batches are applied.
    pub estimated_final_score: f32,
}

/// A single batch of findings to address together.
#[derive(Debug, Clone, Serialize)]
pub struct RedesignBatch {
    /// Priority level (1 = highest).
    pub priority: u32,
    /// Findings to address in this batch.
    pub findings: Vec<Finding>,
    /// Estimated score improvement from this batch.
    pub estimated_improvement: f32,
}

/// Generate a redesign plan from a taste report. Returns `None` if the score
/// is already at or above `threshold`.
pub fn plan_redesign(report: &TasteReport, threshold: f32) -> Option<RedesignPlan> {
    if report.overall_score >= threshold {
        return None;
    }

    let gap = threshold - report.overall_score;

    // Group findings by severity into batches
    let critical: Vec<Finding> = report.findings.iter().filter(|f| f.severity == Severity::Critical).cloned().collect();
    let major: Vec<Finding> = report.findings.iter().filter(|f| f.severity == Severity::Major).cloned().collect();
    let minor: Vec<Finding> = report.findings.iter().filter(|f| f.severity == Severity::Minor).cloned().collect();
    let info: Vec<Finding> = report.findings.iter().filter(|f| f.severity == Severity::Info).cloned().collect();

    let mut batches = Vec::new();
    let total_findings = report.findings.len().max(1) as f32;

    if !critical.is_empty() {
        let improvement = (critical.len() as f32 / total_findings) * gap * 1.5;
        batches.push(RedesignBatch {
            priority: 1,
            findings: critical,
            estimated_improvement: improvement.min(gap * 0.5),
        });
    }
    if !major.is_empty() {
        let improvement = (major.len() as f32 / total_findings) * gap * 1.2;
        batches.push(RedesignBatch {
            priority: 2,
            findings: major,
            estimated_improvement: improvement.min(gap * 0.35),
        });
    }
    if !minor.is_empty() {
        let improvement = (minor.len() as f32 / total_findings) * gap;
        batches.push(RedesignBatch {
            priority: 3,
            findings: minor,
            estimated_improvement: improvement.min(gap * 0.2),
        });
    }
    if !info.is_empty() {
        let improvement = (info.len() as f32 / total_findings) * gap * 0.5;
        batches.push(RedesignBatch {
            priority: 4,
            findings: info,
            estimated_improvement: improvement.min(gap * 0.1),
        });
    }

    let total_improvement: f32 = batches.iter().map(|b| b.estimated_improvement).sum();
    let estimated_final = (report.overall_score + total_improvement).min(100.0);

    Some(RedesignPlan {
        batches,
        estimated_final_score: estimated_final,
    })
}

// ===========================================================================
// Taste Profile
// ===========================================================================

/// A configurable profile that adjusts scoring weights and thresholds
/// to match different design philosophies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasteProfile {
    /// Human-readable profile name.
    pub name: String,
    /// Per-axis weight overrides (axis_name -> weight).
    pub axis_weights: HashMap<String, f32>,
    /// Per-axis score thresholds (axis_name -> minimum acceptable score).
    pub thresholds: HashMap<String, f32>,
    /// Style preference hint for generation.
    pub style_preference: Option<String>,
}

impl TasteProfile {
    /// Balanced default profile — equal weighting across all quality dimensions.
    pub fn default_profile() -> Self {
        Self {
            name: "default".to_string(),
            axis_weights: HashMap::new(),
            thresholds: [
                ("overall".to_string(), 70.0),
                ("color_contrast".to_string(), 60.0),
                ("accessibility_attributes".to_string(), 60.0),
            ]
            .into(),
            style_preference: None,
        }
    }

    /// Minimal profile — focuses on essentials, tolerates fewer polish signals.
    pub fn minimal() -> Self {
        Self {
            name: "minimal".to_string(),
            axis_weights: [
                ("color_contrast".to_string(), 0.10),
                ("spacing_consistency".to_string(), 0.25),
                ("typography_hierarchy".to_string(), 0.20),
                ("component_structure".to_string(), 0.20),
                ("responsive_breakpoints".to_string(), 0.15),
                ("accessibility_attributes".to_string(), 0.10),
            ]
            .into(),
            thresholds: [("overall".to_string(), 55.0)].into(),
            style_preference: Some("minimal".to_string()),
        }
    }

    /// Corporate profile — emphasizes accessibility, consistency, and professionalism.
    pub fn corporate() -> Self {
        Self {
            name: "corporate".to_string(),
            axis_weights: [
                ("color_contrast".to_string(), 0.20),
                ("spacing_consistency".to_string(), 0.20),
                ("typography_hierarchy".to_string(), 0.15),
                ("component_structure".to_string(), 0.10),
                ("responsive_breakpoints".to_string(), 0.15),
                ("accessibility_attributes".to_string(), 0.20),
            ]
            .into(),
            thresholds: [
                ("overall".to_string(), 75.0),
                ("accessibility_attributes".to_string(), 70.0),
                ("color_contrast".to_string(), 70.0),
            ]
            .into(),
            style_preference: Some("corporate".to_string()),
        }
    }

    /// Bold profile — emphasizes visual impact, modern UI, and animations.
    pub fn bold() -> Self {
        Self {
            name: "bold".to_string(),
            axis_weights: [
                ("color_contrast".to_string(), 0.25),
                ("spacing_consistency".to_string(), 0.15),
                ("typography_hierarchy".to_string(), 0.20),
                ("component_structure".to_string(), 0.10),
                ("responsive_breakpoints".to_string(), 0.10),
                ("accessibility_attributes".to_string(), 0.20),
            ]
            .into(),
            thresholds: [
                ("overall".to_string(), 65.0),
                ("color_contrast".to_string(), 65.0),
            ]
            .into(),
            style_preference: Some("bold".to_string()),
        }
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Compute weighted total score for a stage from its axis scores.
fn compute_stage_total(axes: &HashMap<String, AxisScore>) -> f32 {
    let total_weight: f32 = axes.values().map(|a| a.weight).sum();
    if total_weight == 0.0 {
        return 0.0;
    }
    let weighted_sum: f32 = axes.values().map(|a| a.score * a.weight).sum();
    weighted_sum / total_weight
}

/// Check if a string contains a hex color pattern (e.g. #3b82f6) outside of CSS vars.
fn contains_hex_color(content: &str) -> bool {
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '#' {
            let hex: String = chars.clone().take(6).collect();
            if hex.len() >= 3 && hex.chars().take(3).all(|c| c.is_ascii_hexdigit()) {
                return true;
            }
        }
    }
    false
}

/// Find the first line number containing any of the given patterns.
fn find_line_containing(content: &str, patterns: &[&str]) -> Option<usize> {
    for (idx, line) in content.lines().enumerate() {
        if patterns.iter().any(|p| line.contains(p)) {
            return Some(idx + 1);
        }
    }
    None
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context(files: Vec<ProjectFile>) -> TasteContext {
        TasteContext {
            files,
            screenshots: None,
            app_type: "saas".to_string(),
            domain: "fintech".to_string(),
            style_preference: None,
        }
    }

    fn make_file(path: &str, content: &str) -> ProjectFile {
        let ext = path.rsplit('.').next().unwrap_or("").to_string();
        ProjectFile {
            path: path.to_string(),
            content: content.to_string(),
            extension: ext,
        }
    }

    // ── Static Analysis Stage ──────────────────────────────────────────────

    #[test]
    fn static_analysis_scores_good_project() {
        let files = vec![
            make_file("src/globals.css", r#"
                :root {
                    --primary: hsl(220, 80%, 50%);
                    --foreground: hsl(0, 0%, 10%);
                    --background: hsl(0, 0%, 100%);
                    --muted: hsl(0, 0%, 90%);
                    --spacing: 0.5rem;
                }
                .dark { --background: hsl(0, 0%, 5%); }
            "#),
            make_file("src/page.tsx", r#"
                export default function Home() {
                    return (
                        <div className="flex flex-col min-h-screen gap-8">
                            <nav className="flex items-center px-6 py-4" aria-label="Main navigation" role="navigation">
                                <h1 className="font-sans text-2xl font-bold leading-tight text-foreground">Dashboard</h1>
                            </nav>
                            <main className="container mx-auto px-4 py-8 max-w-7xl">
                                <h2 className="text-xl font-semibold text-muted-foreground">Overview</h2>
                                <div className="flex flex-col md:flex-row gap-4">
                                    <div className="p-6 rounded-lg shadow-sm">
                                        <img src="/chart.png" alt="Revenue chart" />
                                        <label htmlFor="search">Search</label>
                                        <input id="search" className="focus-visible:ring-2" />
                                        <span className="sr-only">Screen reader text</span>
                                        <button className="hidden md:block">Desktop only</button>
                                    </div>
                                </div>
                            </main>
                            <meta name="viewport" content="width=device-width, initial-scale=1" />
                        </div>
                    );
                }
            "#),
        ];

        let stage = StaticAnalysisStage;
        let result = stage.score(&make_context(files)).unwrap();

        assert!(result.total > 50.0, "Expected good score, got {}", result.total);
        assert!(result.axes.contains_key("color_contrast"));
        assert!(result.axes.contains_key("spacing_consistency"));
        assert!(result.axes.contains_key("accessibility_attributes"));
    }

    #[test]
    fn static_analysis_penalizes_poor_project() {
        let files = vec![
            make_file("src/page.tsx", r#"
                export default function Page() {
                    return <div>Hello world</div>;
                }
            "#),
        ];

        let stage = StaticAnalysisStage;
        let result = stage.score(&make_context(files)).unwrap();

        assert!(result.total < 50.0, "Expected low score for bare project, got {}", result.total);
        // Should have findings for missing color tokens, spacing, etc.
        let total_findings: usize = result.axes.values().map(|a| a.findings.len()).sum();
        assert!(total_findings > 3, "Expected multiple findings, got {}", total_findings);
    }

    // ── Content Quality Stage ──────────────────────────────────────────────

    #[test]
    fn content_quality_detects_lorem_ipsum() {
        let files = vec![
            make_file("src/page.tsx", r#"
                export default function About() {
                    return (
                        <div>
                            <h1>About Us</h1>
                            <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit.</p>
                        </div>
                    );
                }
            "#),
        ];

        let stage = ContentQualityStage;
        let result = stage.score(&make_context(files)).unwrap();

        let lorem_axis = result.axes.get("no_lorem_ipsum").unwrap();
        assert!(lorem_axis.score < 100.0, "Expected penalty for lorem ipsum");
        assert!(lorem_axis.findings.iter().any(|f| f.severity == Severity::Critical));
    }

    #[test]
    fn content_quality_rewards_complete_states() {
        let files = vec![
            make_file("src/page.tsx", r#"
                import { Skeleton } from './skeleton';
                export default function Dashboard() {
                    const { data, isLoading, isError } = useQuery();
                    if (isLoading) return <Skeleton />;
                    if (isError) return <div>Something went wrong. Please try again.</div>;
                    if (data.length === 0) return <EmptyState>Create your first item</EmptyState>;
                    return <div>{data.map((item) => <Card key={item.id}>{item.name}</Card>)}</div>;
                }
            "#),
            make_file("src/not-found.tsx", r#"
                export default function NotFound() {
                    return <div>404 - Page not found</div>;
                }
            "#),
        ];

        let stage = ContentQualityStage;
        let result = stage.score(&make_context(files)).unwrap();

        assert!(result.total > 60.0, "Expected good content quality score, got {}", result.total);
    }

    // ── Code Quality Stage ──────────────────────────────────────────────

    #[test]
    fn code_quality_penalizes_inline_styles() {
        let files = vec![
            make_file("src/page.tsx", r#"
                export default function Page() {
                    return (
                        <div style={{ color: 'red', marginTop: '10px' }}>
                            <span style={{ fontSize: '14px' }}>Hello</span>
                        </div>
                    );
                }
            "#),
        ];

        let stage = CodeQualityStage;
        let result = stage.score(&make_context(files)).unwrap();

        let inline_axis = result.axes.get("no_inline_styles").unwrap();
        assert!(inline_axis.score < 100.0, "Expected penalty for inline styles");
        assert!(!inline_axis.findings.is_empty());
    }

    #[test]
    fn code_quality_detects_hardcoded_urls() {
        let files = vec![
            make_file("src/api.ts", r#"
                const API_URL = "http://localhost:3000/api";
                export async function fetchData() {
                    return fetch(API_URL);
                }
            "#),
        ];

        let stage = CodeQualityStage;
        let result = stage.score(&make_context(files.clone())).unwrap();

        let hardcoded_axis = result.axes.get("no_hardcoded_values").unwrap();
        assert!(hardcoded_axis.score < 100.0, "Expected penalty for hardcoded URL");
    }

    // ── Visual Analysis Stage ──────────────────────────────────────────────

    #[test]
    fn visual_analysis_returns_defaults_without_screenshots() {
        let stage = VisualAnalysisStage;
        let context = make_context(vec![]);
        let result = stage.score(&context).unwrap();

        assert_eq!(result.stage, "visual_analysis");
        assert!(result.total > 0.0, "Should return non-zero default");
        assert!(result.axes.contains_key("visual_appeal"));
    }

    #[test]
    fn visual_analysis_acknowledges_screenshots() {
        // When screenshots are provided AND files have CSS, the visual analysis
        // runs code-based heuristics on Tailwind/CSS patterns.
        let mut context = make_context(vec![
            make_file("src/page.tsx", r#"<div className="flex gap-4 shadow rounded hover:bg-gray-100 transition">"#),
        ]);
        context.screenshots = Some(vec![
            Screenshot {
                url: "http://localhost/screenshot1.png".to_string(),
                page: "/".to_string(),
                viewport: "1920x1080".to_string(),
            },
        ]);

        let stage = VisualAnalysisStage;
        let result = stage.score(&context).unwrap();

        // Visual appeal should get a boost from shadow, rounded, hover, transition
        let appeal = result.axes.get("visual_appeal").unwrap();
        assert!(appeal.score > 55.0, "Visual appeal should be boosted by design tokens, got {}", appeal.score);
    }

    // ── Pipeline end-to-end ──────────────────────────────────────────────

    #[test]
    fn pipeline_scores_end_to_end() {
        let files = vec![
            make_file("src/globals.css", ":root { --primary: hsl(220, 80%, 50%); }"),
            make_file("src/page.tsx", r#"
                export default function Home() {
                    return (
                        <div className="flex flex-col gap-4 px-6 py-8 max-w-7xl mx-auto">
                            <h1 className="text-2xl font-bold">Welcome</h1>
                            <h2 className="text-lg font-semibold">Features</h2>
                        </div>
                    );
                }
            "#),
        ];

        let pipeline = TastePipeline::default_pipeline();
        let report = pipeline.score(&make_context(files));

        assert!(report.overall_score > 0.0, "Score should be positive");
        assert_eq!(report.stages.len(), 4, "All 4 stages should run");
        assert!(!report.findings.is_empty(), "Should have some findings");

        // Findings should be sorted by severity
        for window in report.findings.windows(2) {
            assert!(window[0].severity <= window[1].severity, "Findings should be sorted by severity");
        }
    }

    #[test]
    fn pipeline_with_plan_generates_redesign() {
        let files = vec![
            make_file("src/page.tsx", r#"
                export default function Page() {
                    return <div>Hello</div>;
                }
            "#),
        ];

        let pipeline = TastePipeline::default_pipeline();
        let report = pipeline.score_with_plan(&make_context(files), 90.0);

        assert!(report.redesign_plan.is_some(), "Should generate a plan when below threshold");
        let plan = report.redesign_plan.unwrap();
        assert!(!plan.batches.is_empty(), "Plan should have batches");
        assert!(plan.estimated_final_score > report.overall_score, "Plan should improve score");

        // Batches should be in priority order
        for window in plan.batches.windows(2) {
            assert!(window[0].priority <= window[1].priority);
        }
    }

    #[test]
    fn pipeline_no_plan_when_above_threshold() {
        let pipeline = TastePipeline::default_pipeline();

        // Construct a well-built project that scores high
        let files = vec![
            make_file("src/globals.css", r#"
                :root {
                    --primary: hsl(220, 80%, 50%); --foreground: hsl(0, 0%, 10%);
                    --background: hsl(0, 0%, 100%); --muted: hsl(0, 0%, 90%);
                    --accent: hsl(240, 70%, 60%); --spacing: 1rem;
                }
                .dark { --foreground: hsl(0, 0%, 95%); }
            "#),
            make_file("src/components/layout.tsx", r#"
                interface LayoutProps { children: React.ReactNode; }
                export default function Layout({ children }: LayoutProps) {
                    return (
                        <div className="flex flex-col min-h-screen">
                            <nav className="flex items-center px-6 py-4 gap-4" role="navigation" aria-label="Main">
                                <h1 className="text-xl font-bold text-foreground font-sans leading-tight">App</h1>
                                <span className="sr-only">Navigation</span>
                            </nav>
                            <main className="container mx-auto max-w-7xl px-4 py-8 flex flex-col md:flex-row gap-6">
                                {children}
                            </main>
                            <meta name="viewport" content="width=device-width, initial-scale=1" />
                        </div>
                    );
                }
            "#),
            make_file("src/page.tsx", r#"
                import { Skeleton } from './skeleton';
                export default function Home() {
                    const { data, isLoading, isError } = useQuery();
                    if (isLoading) return <Skeleton />;
                    if (isError) return <div>Something went wrong. Please try again.</div>;
                    if (data.length === 0) return <div>No results found. <button>Create your first item</button></div>;
                    return (
                        <div className="flex flex-col gap-4 sm:gap-6 md:gap-8">
                            <h1 className="text-3xl font-bold text-foreground">Dashboard</h1>
                            <h2 className="text-xl font-semibold text-muted-foreground leading-relaxed">Your overview</h2>
                            <div className="hidden md:block">Desktop content</div>
                            <img src="/hero.png" alt="Dashboard overview illustration" />
                            <label htmlFor="search">Search items</label>
                            <input id="search" className="focus-visible:ring-2 focus-visible:ring-offset-2" />
                            {data.map((item) => <div key={item.id} className="p-4 rounded-lg">{item.name}</div>)}
                        </div>
                    );
                }
            "#),
            make_file("src/not-found.tsx", "export default function NotFound() { return <div>404 - This page doesn't exist</div>; }"),
        ];

        let report = pipeline.score_with_plan(&make_context(files), 30.0);
        assert!(report.redesign_plan.is_none(), "Should not need a plan when score {} >= 30", report.overall_score);
    }

    // ── Redesign Plan ──────────────────────────────────────────────────────

    #[test]
    fn plan_redesign_groups_by_severity() {
        let report = TasteReport {
            overall_score: 40.0,
            stages: vec![],
            findings: vec![
                Finding { severity: Severity::Critical, description: "crit".into(), file: None, line: None, auto_fixable: true, fix_suggestion: None },
                Finding { severity: Severity::Major, description: "major".into(), file: None, line: None, auto_fixable: true, fix_suggestion: None },
                Finding { severity: Severity::Minor, description: "minor".into(), file: None, line: None, auto_fixable: false, fix_suggestion: None },
                Finding { severity: Severity::Info, description: "info".into(), file: None, line: None, auto_fixable: false, fix_suggestion: None },
            ],
            redesign_plan: None,
        };

        let plan = plan_redesign(&report, 80.0).unwrap();
        assert_eq!(plan.batches.len(), 4, "One batch per severity level");
        assert_eq!(plan.batches[0].priority, 1);
        assert_eq!(plan.batches[0].findings[0].severity, Severity::Critical);
        assert_eq!(plan.batches[1].priority, 2);
        assert_eq!(plan.batches[3].priority, 4);
        assert!(plan.estimated_final_score > 40.0);
    }

    // ── Profiles ────────────────────────────────────────────────────────────

    #[test]
    fn profiles_have_distinct_configurations() {
        let default = TasteProfile::default_profile();
        let minimal = TasteProfile::minimal();
        let corporate = TasteProfile::corporate();
        let bold = TasteProfile::bold();

        assert_eq!(default.name, "default");
        assert_eq!(minimal.name, "minimal");
        assert_eq!(corporate.name, "corporate");
        assert_eq!(bold.name, "bold");

        // Corporate should have higher accessibility threshold
        assert!(corporate.thresholds.get("accessibility_attributes").unwrap_or(&0.0) > minimal.thresholds.get("accessibility_attributes").unwrap_or(&0.0));

        // Minimal should have lower overall threshold
        assert!(*minimal.thresholds.get("overall").unwrap() < *corporate.thresholds.get("overall").unwrap());

        // Bold and corporate should have style preferences
        assert!(corporate.style_preference.is_some());
        assert!(bold.style_preference.is_some());
    }

    // ── Finding severity sorting ────────────────────────────────────────────

    #[test]
    fn severity_ordering_is_correct() {
        assert!(Severity::Critical < Severity::Major);
        assert!(Severity::Major < Severity::Minor);
        assert!(Severity::Minor < Severity::Info);

        let mut findings = [
            Finding { severity: Severity::Info, description: "info".into(), file: None, line: None, auto_fixable: false, fix_suggestion: None },
            Finding { severity: Severity::Critical, description: "crit".into(), file: None, line: None, auto_fixable: false, fix_suggestion: None },
            Finding { severity: Severity::Minor, description: "minor".into(), file: None, line: None, auto_fixable: false, fix_suggestion: None },
            Finding { severity: Severity::Major, description: "major".into(), file: None, line: None, auto_fixable: false, fix_suggestion: None },
        ];

        findings.sort_by(|a, b| a.severity.cmp(&b.severity));

        assert_eq!(findings[0].severity, Severity::Critical);
        assert_eq!(findings[1].severity, Severity::Major);
        assert_eq!(findings[2].severity, Severity::Minor);
        assert_eq!(findings[3].severity, Severity::Info);
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    #[test]
    fn compute_stage_total_weighted_average() {
        let mut axes = HashMap::new();
        axes.insert("a".to_string(), AxisScore { score: 100.0, weight: 0.5, findings: vec![] });
        axes.insert("b".to_string(), AxisScore { score: 0.0, weight: 0.5, findings: vec![] });
        let total = compute_stage_total(&axes);
        assert!((total - 50.0).abs() < 0.01);
    }

    #[test]
    fn find_line_containing_works() {
        let content = "line one\nline two\nthe target\nline four";
        assert_eq!(find_line_containing(content, &["target"]), Some(3));
        assert_eq!(find_line_containing(content, &["missing"]), None);
    }
}

// ===========================================================================
// Legacy simple scoring API
//
// Deterministic UX quality scoring for generated apps. Scores on 6 axes:
// Visual Quality, UX Clarity, Conversion, Modern UI, Accessibility, Responsiveness.
// Each axis scores 0-100. Overall taste score is the weighted average.
// ===========================================================================

use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasteScore {
    pub overall: u32,
    pub visual_quality: SimpleAxisScore,
    pub ux_clarity: SimpleAxisScore,
    pub conversion: SimpleAxisScore,
    pub modern_ui: SimpleAxisScore,
    pub accessibility: SimpleAxisScore,
    pub responsiveness: SimpleAxisScore,
    pub improvements: Vec<TasteImprovement>,
    pub total_files_scanned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleAxisScore {
    pub score: u32,
    pub weight: f32,
    pub signals_found: Vec<String>,
    pub signals_missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasteImprovement {
    pub axis: String,
    pub priority: String,
    pub description: String,
    pub file: Option<String>,
    pub suggestion: String,
}

/// Score the taste/quality of a generated project.
pub fn score_project(project_dir: &Path) -> TasteScore {
    let files = collect_ui_files(project_dir);
    let total_files = files.len();

    let mut all_content = String::new();
    let mut file_contents: Vec<(String, String)> = Vec::new();

    for path in &files {
        if let Ok(content) = std::fs::read_to_string(path) {
            let rel = path
                .strip_prefix(project_dir)
                .unwrap_or(path)
                .display()
                .to_string();
            all_content.push_str(&content);
            all_content.push('\n');
            file_contents.push((rel, content));
        }
    }

    let visual = score_visual_quality(&all_content, &file_contents);
    let ux = score_ux_clarity(&all_content, &file_contents);
    let conversion = score_conversion(&all_content, &file_contents);
    let modern = score_modern_ui(&all_content, &file_contents);
    let accessibility_score = score_accessibility_simple(&all_content, &file_contents);
    let responsiveness_score = score_responsiveness_simple(&all_content, &file_contents);

    let overall = (visual.score as f32 * visual.weight
        + ux.score as f32 * ux.weight
        + conversion.score as f32 * conversion.weight
        + modern.score as f32 * modern.weight
        + accessibility_score.score as f32 * accessibility_score.weight
        + responsiveness_score.score as f32 * responsiveness_score.weight)
        as u32;

    let mut improvements = Vec::new();
    collect_improvements_simple(&visual, "visual_quality", &mut improvements);
    collect_improvements_simple(&ux, "ux_clarity", &mut improvements);
    collect_improvements_simple(&conversion, "conversion", &mut improvements);
    collect_improvements_simple(&modern, "modern_ui", &mut improvements);
    collect_improvements_simple(&accessibility_score, "accessibility", &mut improvements);
    collect_improvements_simple(&responsiveness_score, "responsiveness", &mut improvements);

    improvements.sort_by(|a, b| priority_ord_simple(&a.priority).cmp(&priority_ord_simple(&b.priority)));

    TasteScore {
        overall,
        visual_quality: visual,
        ux_clarity: ux,
        conversion,
        modern_ui: modern,
        accessibility: accessibility_score,
        responsiveness: responsiveness_score,
        improvements,
        total_files_scanned: total_files,
    }
}

// ---------------------------------------------------------------------------
// Simple axis scoring helpers
// ---------------------------------------------------------------------------

fn score_visual_quality(all: &str, _files: &[(String, String)]) -> SimpleAxisScore {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    check_simple(all, &["--radius", "border-radius", "rounded"], "Border radius tokens", &mut found, &mut missing);
    check_simple(all, &["--spacing", "gap-", "space-", "px-", "py-"], "Spacing system", &mut found, &mut missing);
    check_simple(all, &["--color", "--primary", "--foreground", "hsl("], "Color tokens/variables", &mut found, &mut missing);
    check_simple(all, &["font-sans", "font-mono", "font-serif", "--font-"], "Typography system", &mut found, &mut missing);
    check_simple(all, &["shadow", "shadow-sm", "shadow-lg", "drop-shadow"], "Shadow depth", &mut found, &mut missing);
    check_simple(all, &["bg-gradient", "from-", "to-", "linear-gradient"], "Gradients", &mut found, &mut missing);

    let score = ((found.len() as f32 / (found.len() + missing.len()).max(1) as f32) * 100.0) as u32;
    SimpleAxisScore { score, weight: 0.20, signals_found: found, signals_missing: missing }
}

fn score_ux_clarity(all: &str, _files: &[(String, String)]) -> SimpleAxisScore {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    check_simple(all, &["<nav", "navigation", "sidebar", "navbar"], "Navigation component", &mut found, &mut missing);
    check_simple(all, &["loading", "isLoading", "spinner", "skeleton", "Skeleton"], "Loading states", &mut found, &mut missing);
    check_simple(all, &["error", "Error", "catch", "ErrorBoundary"], "Error handling UI", &mut found, &mut missing);
    check_simple(all, &["empty", "no-data", "No results", "nothing here", "EmptyState"], "Empty states", &mut found, &mut missing);
    check_simple(all, &["breadcrumb", "Breadcrumb"], "Breadcrumbs", &mut found, &mut missing);
    check_simple(all, &["toast", "Toast", "notification", "Notification", "snackbar"], "User feedback (toasts)", &mut found, &mut missing);
    check_simple(all, &["confirm", "dialog", "Dialog", "modal", "Modal"], "Confirmation dialogs", &mut found, &mut missing);

    let score = ((found.len() as f32 / (found.len() + missing.len()).max(1) as f32) * 100.0) as u32;
    SimpleAxisScore { score, weight: 0.25, signals_found: found, signals_missing: missing }
}

fn score_conversion(all: &str, _files: &[(String, String)]) -> SimpleAxisScore {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    check_simple(all, &["cta", "CTA", "Get Started", "Sign Up", "Try Free", "Start Now"], "Call to action", &mut found, &mut missing);
    check_simple(all, &["testimonial", "review", "rating", "stars"], "Social proof", &mut found, &mut missing);
    check_simple(all, &["trust", "secure", "encrypted", "privacy", "badge"], "Trust signals", &mut found, &mut missing);
    check_simple(all, &["pricing", "plan", "tier", "monthly", "yearly"], "Pricing section", &mut found, &mut missing);
    check_simple(all, &["hero", "Hero", "headline"], "Hero section", &mut found, &mut missing);

    let score = ((found.len() as f32 / (found.len() + missing.len()).max(1) as f32) * 100.0) as u32;
    SimpleAxisScore { score, weight: 0.10, signals_found: found, signals_missing: missing }
}

fn score_modern_ui(all: &str, _files: &[(String, String)]) -> SimpleAxisScore {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    check_simple(all, &["transition", "duration-", "ease-", "animate-"], "CSS transitions", &mut found, &mut missing);
    check_simple(all, &["hover:", "group-hover:", "focus-visible:"], "Hover/focus interactions", &mut found, &mut missing);
    check_simple(all, &["backdrop-blur", "glass", "bg-opacity", "bg-white/"], "Glass morphism / blur", &mut found, &mut missing);
    check_simple(all, &["dark:", "dark-mode", "theme-"], "Dark mode support", &mut found, &mut missing);
    check_simple(all, &["framer-motion", "motion.", "animate", "@keyframes"], "Animations", &mut found, &mut missing);
    check_simple(all, &["lucide", "heroicons", "icon", "Icon", "svg"], "Icon system", &mut found, &mut missing);

    let score = ((found.len() as f32 / (found.len() + missing.len()).max(1) as f32) * 100.0) as u32;
    SimpleAxisScore { score, weight: 0.15, signals_found: found, signals_missing: missing }
}

fn score_accessibility_simple(all: &str, _files: &[(String, String)]) -> SimpleAxisScore {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    check_simple(all, &["alt=", "alt ="], "Image alt text", &mut found, &mut missing);
    check_simple(all, &["aria-label", "aria-describedby", "aria-hidden"], "ARIA attributes", &mut found, &mut missing);
    check_simple(all, &["role=", "role ="], "ARIA roles", &mut found, &mut missing);
    check_simple(all, &["focus-visible", "focus-within", "outline-", "ring-"], "Focus indicators", &mut found, &mut missing);
    check_simple(all, &["<label", "htmlFor", "for="], "Form labels", &mut found, &mut missing);
    check_simple(all, &["sr-only", "screen-reader", "visually-hidden"], "Screen reader content", &mut found, &mut missing);

    let score = ((found.len() as f32 / (found.len() + missing.len()).max(1) as f32) * 100.0) as u32;
    SimpleAxisScore { score, weight: 0.15, signals_found: found, signals_missing: missing }
}

fn score_responsiveness_simple(all: &str, _files: &[(String, String)]) -> SimpleAxisScore {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    check_simple(all, &["sm:", "md:", "lg:", "xl:", "@media"], "Responsive breakpoints", &mut found, &mut missing);
    check_simple(all, &["flex", "grid", "flex-col", "grid-cols"], "Flex/grid layout", &mut found, &mut missing);
    check_simple(all, &["max-w-", "container", "mx-auto"], "Container constraints", &mut found, &mut missing);
    check_simple(all, &["w-full", "min-h-screen", "overflow-"], "Full-width handling", &mut found, &mut missing);
    check_simple(all, &["hidden sm:", "block md:", "lg:hidden", "md:flex"], "Responsive visibility", &mut found, &mut missing);
    check_simple(all, &["viewport", "meta name=\"viewport\""], "Viewport meta", &mut found, &mut missing);

    let score = ((found.len() as f32 / (found.len() + missing.len()).max(1) as f32) * 100.0) as u32;
    SimpleAxisScore { score, weight: 0.15, signals_found: found, signals_missing: missing }
}

fn check_simple(content: &str, patterns: &[&str], signal_name: &str, found: &mut Vec<String>, missing: &mut Vec<String>) {
    if patterns.iter().any(|p| content.contains(p)) {
        found.push(signal_name.to_string());
    } else {
        missing.push(signal_name.to_string());
    }
}

fn collect_improvements_simple(axis: &SimpleAxisScore, axis_name: &str, improvements: &mut Vec<TasteImprovement>) {
    for signal in &axis.signals_missing {
        let priority = if axis.score < 30 { "critical" } else if axis.score < 60 { "high" } else { "medium" };
        improvements.push(TasteImprovement {
            axis: axis_name.to_string(),
            priority: priority.to_string(),
            description: format!("Missing: {}", signal),
            file: None,
            suggestion: format!("Add {} to improve {} score", signal, axis_name),
        });
    }
}

fn priority_ord_simple(p: &str) -> u8 {
    match p {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

fn collect_ui_files(dir: &Path) -> Vec<std::path::PathBuf> {
    crate::file_utils::collect_files_by_ext(dir, &["tsx", "jsx", "css", "html"])
}

#[cfg(test)]
mod legacy_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scores_basic_project() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/page.tsx"),
            r#"
            export default function Home() {
                return (
                    <div className="flex flex-col min-h-screen bg-gradient-to-b from-blue-50 to-white">
                        <nav className="flex items-center p-4 shadow-sm">
                            <h1 className="font-sans text-xl">My App</h1>
                        </nav>
                        <main className="container mx-auto px-4 py-8">
                            <div className="rounded-lg bg-white shadow-lg p-6 transition duration-200 hover:shadow-xl">
                                <h2 className="text-2xl font-bold">Hero</h2>
                                <button className="bg-blue-600 text-white px-4 py-2 rounded focus-visible:ring-2">
                                    Get Started
                                </button>
                            </div>
                        </main>
                    </div>
                );
            }
            "#,
        ).unwrap();
        fs::write(
            dir.path().join("src/globals.css"),
            ":root { --primary: hsl(220, 80%, 50%); --radius: 0.5rem; }",
        ).unwrap();

        let score = score_project(dir.path());
        assert!(score.overall > 30, "Expected decent score, got {}", score.overall);
        assert!(score.visual_quality.score > 50);
    }
}
