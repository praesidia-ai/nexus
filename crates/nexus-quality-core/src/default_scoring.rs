//! Default heuristic implementation of [`QualityScorer`].
//!
//! Evaluates generated project files using deterministic rules:
//! - File size checks (not too small, not too large)
//! - HTML structure (has doctype, viewport meta, semantic elements)
//! - CSS quality (responsive breakpoints, custom properties usage)
//! - Accessibility basics (alt attributes, ARIA labels)

use async_trait::async_trait;

use super::scoring::*;

/// Built-in heuristic quality scorer.
///
/// Reads generated files from disk and applies deterministic rules to
/// produce a [`TasteScore`]. Does not use LLM calls.
pub struct HeuristicQualityScorer {
    min_threshold: f32,
}

impl HeuristicQualityScorer {
    /// Create a scorer with a custom minimum passing threshold.
    pub fn new(min_threshold: f32) -> Self {
        Self { min_threshold }
    }

    fn score_html(&self, content: &str, file: &str) -> Vec<(DimensionScore, Vec<QualityIssue>)> {
        let mut results = Vec::new();
        let mut layout_score = 80.0_f32;
        let mut layout_details = Vec::new();
        let mut issues = Vec::new();

        // Check doctype
        if !content.to_lowercase().contains("<!doctype html>") {
            layout_score -= 15.0;
            issues.push(QualityIssue {
                severity: IssueSeverity::Warning,
                dimension: "layout".to_string(),
                description: "Missing <!DOCTYPE html> declaration".to_string(),
                location: Some(IssueLocation {
                    file: file.to_string(),
                    line: Some(1),
                    selector: None,
                }),
                fix_suggestion: Some("Add <!DOCTYPE html> at the top of the file".to_string()),
            });
        } else {
            layout_details.push("Has DOCTYPE declaration".to_string());
        }

        // Check viewport meta
        if !content.contains("viewport") {
            layout_score -= 10.0;
            issues.push(QualityIssue {
                severity: IssueSeverity::Warning,
                dimension: "layout".to_string(),
                description: "Missing viewport meta tag".to_string(),
                location: Some(IssueLocation {
                    file: file.to_string(),
                    line: None,
                    selector: Some("head".to_string()),
                }),
                fix_suggestion: Some(
                    "Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">".to_string(),
                ),
            });
        } else {
            layout_details.push("Has viewport meta tag".to_string());
        }

        // Check semantic HTML
        let semantic_tags = ["<header", "<main", "<footer", "<nav", "<section", "<article"];
        let semantic_count = semantic_tags.iter().filter(|t| content.contains(*t)).count();
        if semantic_count >= 3 {
            layout_score += 5.0;
            layout_details.push(format!("Uses {semantic_count} semantic HTML elements"));
        } else if semantic_count == 0 {
            layout_score -= 10.0;
            issues.push(QualityIssue {
                severity: IssueSeverity::Info,
                dimension: "layout".to_string(),
                description: "No semantic HTML elements found".to_string(),
                location: Some(IssueLocation {
                    file: file.to_string(),
                    line: None,
                    selector: None,
                }),
                fix_suggestion: Some("Use <header>, <main>, <footer>, <nav> instead of generic <div>".to_string()),
            });
        }

        // Accessibility: check for alt attributes on images
        let img_count = content.matches("<img").count();
        let alt_count = content.matches("alt=").count();
        let mut a11y_score = 75.0_f32;
        let mut a11y_details = Vec::new();

        if img_count > 0 && alt_count < img_count {
            a11y_score -= 20.0;
            issues.push(QualityIssue {
                severity: IssueSeverity::Critical,
                dimension: "accessibility".to_string(),
                description: format!("{} image(s) missing alt attribute", img_count - alt_count),
                location: Some(IssueLocation {
                    file: file.to_string(),
                    line: None,
                    selector: Some("img".to_string()),
                }),
                fix_suggestion: Some("Add descriptive alt attributes to all <img> tags".to_string()),
            });
        } else if img_count > 0 {
            a11y_details.push("All images have alt attributes".to_string());
        }

        if content.contains("aria-") {
            a11y_score += 10.0;
            a11y_details.push("Uses ARIA attributes".to_string());
        }

        results.push((
            DimensionScore {
                dimension: "layout".to_string(),
                score: layout_score.clamp(0.0, 100.0),
                weight: 0.3,
                details: layout_details,
            },
            issues.clone(),
        ));

        results.push((
            DimensionScore {
                dimension: "accessibility".to_string(),
                score: a11y_score.clamp(0.0, 100.0),
                weight: 0.2,
                details: a11y_details,
            },
            issues,
        ));

        results
    }

    fn score_css(&self, content: &str, file: &str) -> Vec<(DimensionScore, Vec<QualityIssue>)> {
        let mut issues = Vec::new();
        let mut score = 75.0_f32;
        let mut details = Vec::new();

        // Check for responsive breakpoints
        let media_count = content.matches("@media").count();
        if media_count >= 2 {
            score += 10.0;
            details.push(format!("Has {media_count} media queries for responsive design"));
        } else if media_count == 0 {
            score -= 10.0;
            issues.push(QualityIssue {
                severity: IssueSeverity::Warning,
                dimension: "responsiveness".to_string(),
                description: "No @media queries found — may not be responsive".to_string(),
                location: Some(IssueLocation {
                    file: file.to_string(),
                    line: None,
                    selector: None,
                }),
                fix_suggestion: Some("Add media queries for tablet (768px) and mobile (480px) breakpoints".to_string()),
            });
        }

        // Check for CSS custom properties
        if content.contains("--") && content.contains("var(") {
            score += 5.0;
            details.push("Uses CSS custom properties for theming".to_string());
        }

        // Check for !important abuse
        let important_count = content.matches("!important").count();
        if important_count > 5 {
            score -= 10.0;
            issues.push(QualityIssue {
                severity: IssueSeverity::Warning,
                dimension: "code_quality".to_string(),
                description: format!("Excessive !important usage ({important_count} occurrences)"),
                location: Some(IssueLocation {
                    file: file.to_string(),
                    line: None,
                    selector: None,
                }),
                fix_suggestion: Some("Reduce !important usage by improving selector specificity".to_string()),
            });
        }

        vec![(
            DimensionScore {
                dimension: "styling".to_string(),
                score: score.clamp(0.0, 100.0),
                weight: 0.25,
                details,
            },
            issues,
        )]
    }

    fn compute_grade(overall: f32) -> String {
        match overall as u32 {
            95..=100 => "A+".to_string(),
            90..=94 => "A".to_string(),
            85..=89 => "A-".to_string(),
            80..=84 => "B+".to_string(),
            75..=79 => "B".to_string(),
            70..=74 => "B-".to_string(),
            65..=69 => "C+".to_string(),
            60..=64 => "C".to_string(),
            55..=59 => "C-".to_string(),
            50..=54 => "D".to_string(),
            _ => "F".to_string(),
        }
    }
}

impl Default for HeuristicQualityScorer {
    fn default() -> Self {
        Self::new(70.0)
    }
}

#[async_trait]
impl QualityScorer for HeuristicQualityScorer {
    async fn score(
        &self,
        project_dir: &str,
        files: &[String],
    ) -> Result<TasteScore, Box<dyn std::error::Error + Send + Sync>> {
        let mut all_dimensions: Vec<DimensionScore> = Vec::new();
        let mut all_issues: Vec<QualityIssue> = Vec::new();

        for file in files {
            let path = std::path::Path::new(project_dir).join(file);
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue, // skip unreadable files
            };

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            match ext {
                "html" | "htm" => {
                    for (dim, issues) in self.score_html(&content, file) {
                        all_dimensions.push(dim);
                        all_issues.extend(issues);
                    }
                }
                "css" => {
                    for (dim, issues) in self.score_css(&content, file) {
                        all_dimensions.push(dim);
                        all_issues.extend(issues);
                    }
                }
                "tsx" | "jsx" => {
                    // Basic check: treat as HTML-like for layout scoring
                    for (dim, issues) in self.score_html(&content, file) {
                        all_dimensions.push(dim);
                        all_issues.extend(issues);
                    }
                }
                _ => {} // skip unknown file types
            }
        }

        // If no files were scored, return a default passing score
        if all_dimensions.is_empty() {
            return Ok(TasteScore {
                overall: 50.0,
                dimensions: vec![DimensionScore {
                    dimension: "general".to_string(),
                    score: 50.0,
                    weight: 1.0,
                    details: vec!["No scorable files found".to_string()],
                }],
                issues: Vec::new(),
                suggestions: vec!["Add HTML/CSS files to enable quality scoring".to_string()],
                grade: "D".to_string(),
                passes_threshold: false,
            });
        }

        // Weighted average
        let total_weight: f32 = all_dimensions.iter().map(|d| d.weight).sum();
        let overall = if total_weight > 0.0 {
            all_dimensions
                .iter()
                .map(|d| d.score * d.weight)
                .sum::<f32>()
                / total_weight
        } else {
            0.0
        };

        let grade = Self::compute_grade(overall);
        let passes_threshold = overall >= self.min_threshold;

        // Deduplicate issues
        let mut seen = std::collections::HashSet::new();
        all_issues.retain(|issue| seen.insert(issue.description.clone()));

        let suggestions: Vec<String> = all_issues
            .iter()
            .filter_map(|i| i.fix_suggestion.clone())
            .collect();

        Ok(TasteScore {
            overall,
            dimensions: all_dimensions,
            issues: all_issues,
            suggestions,
            grade,
            passes_threshold,
        })
    }

    fn threshold(&self) -> f32 {
        self.min_threshold
    }
}
