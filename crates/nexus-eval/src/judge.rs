//! LLM-as-judge — heuristic local fallback for scoring text quality.
//!
//! The [`LlmJudge`] evaluates generated text against a set of criteria.
//! In offline mode it uses keyword-based heuristics; when a live LLM
//! endpoint is available the judge can be extended to call it.

use serde::{Deserialize, Serialize};

/// Scores text against criteria using heuristic analysis.
///
/// This is the local fallback judge. It performs structural and keyword
/// checks rather than calling an LLM, making it deterministic and free.
pub struct LlmJudge;

/// Score for a single criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionScore {
    /// The criterion that was evaluated.
    pub criterion: String,
    /// Numeric score (0.0-1.0).
    pub score: f32,
    /// Brief explanation of the score.
    pub explanation: String,
}

/// Aggregated judge score across all criteria.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeScore {
    /// Per-criterion scores.
    pub scores: Vec<CriterionScore>,
    /// Overall score (average of criteria scores).
    pub overall: f32,
}

/// Result of comparing two texts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    /// Which text won ("a" or "b").
    pub winner: String,
    /// Score for text A.
    pub scores_a: f32,
    /// Score for text B.
    pub scores_b: f32,
    /// Human-readable reasoning.
    pub reasoning: String,
}

impl LlmJudge {
    /// Create a new judge instance.
    pub fn new() -> Self {
        Self
    }

    /// Score a piece of text against multiple criteria using heuristics.
    ///
    /// Each criterion is evaluated independently. The overall score is the
    /// arithmetic mean of all criterion scores.
    pub fn judge_text(&self, text: &str, criteria: &[String]) -> JudgeScore {
        let scores: Vec<CriterionScore> = criteria
            .iter()
            .map(|c| self.score_criterion(text, c))
            .collect();

        let overall = if scores.is_empty() {
            0.0
        } else {
            scores.iter().map(|s| s.score).sum::<f32>() / scores.len() as f32
        };

        JudgeScore { scores, overall }
    }

    /// Compare two texts and determine a winner based on criteria.
    pub fn compare(
        &self,
        a: &str,
        b: &str,
        criteria: &[String],
    ) -> ComparisonResult {
        let score_a = self.judge_text(a, criteria);
        let score_b = self.judge_text(b, criteria);

        let (winner, reasoning) = if score_a.overall > score_b.overall {
            (
                "a".to_string(),
                format!(
                    "Text A scored {:.2} vs Text B {:.2}. A is stronger overall.",
                    score_a.overall, score_b.overall
                ),
            )
        } else if score_b.overall > score_a.overall {
            (
                "b".to_string(),
                format!(
                    "Text B scored {:.2} vs Text A {:.2}. B is stronger overall.",
                    score_b.overall, score_a.overall
                ),
            )
        } else {
            (
                "tie".to_string(),
                format!(
                    "Both texts scored {:.2}. No clear winner.",
                    score_a.overall
                ),
            )
        };

        ComparisonResult {
            winner,
            scores_a: score_a.overall,
            scores_b: score_b.overall,
            reasoning,
        }
    }

    /// Heuristic scoring for a single criterion.
    ///
    /// Uses keyword matching, structural analysis, and length checks
    /// to approximate quality along the given dimension.
    fn score_criterion(&self, text: &str, criterion: &str) -> CriterionScore {
        let crit_lower = criterion.to_lowercase();
        let text_lower = text.to_lowercase();
        let word_count = text.split_whitespace().count();

        let (score, explanation) = if crit_lower.contains("clarity")
            || crit_lower.contains("readability")
        {
            // Clarity: moderate length, short sentences, common words.
            let avg_sentence_len = Self::avg_sentence_length(text);
            let s = if avg_sentence_len <= 20.0 && word_count >= 10 {
                0.8
            } else if avg_sentence_len <= 30.0 {
                0.6
            } else {
                0.4
            };
            (s, format!("avg sentence length: {avg_sentence_len:.1} words"))
        } else if crit_lower.contains("completeness") || crit_lower.contains("thorough") {
            // Completeness: longer is generally more complete (with diminishing returns).
            let s = (word_count as f32 / 100.0).min(1.0) * 0.9;
            (s, format!("{word_count} words — longer responses tend to be more complete"))
        } else if crit_lower.contains("accuracy") || crit_lower.contains("correct") {
            // Accuracy: presence of technical terms as a proxy.
            let tech_words = [
                "function", "class", "type", "interface", "module", "import",
                "export", "async", "await", "return", "const", "let",
            ];
            let hits = tech_words
                .iter()
                .filter(|w| text_lower.contains(**w))
                .count();
            let s = (hits as f32 / 4.0).min(1.0) * 0.85;
            (s, format!("{hits} technical terms detected"))
        } else if crit_lower.contains("structure") || crit_lower.contains("organization") {
            // Structure: headings, lists, paragraphs.
            let has_sections = text.contains('\n') && text.lines().count() > 3;
            let has_bullets = text.contains("- ") || text.contains("* ");
            let s = match (has_sections, has_bullets) {
                (true, true) => 0.9,
                (true, false) | (false, true) => 0.65,
                (false, false) => 0.35,
            };
            (s, format!("sections={has_sections}, bullets={has_bullets}"))
        } else if crit_lower.contains("creativity") || crit_lower.contains("originality") {
            // Creativity: vocabulary diversity (unique words / total words).
            let unique: std::collections::HashSet<&str> =
                text_lower.split_whitespace().collect();
            let diversity = if word_count == 0 {
                0.0
            } else {
                unique.len() as f32 / word_count as f32
            };
            let s = diversity.min(1.0) * 0.85;
            (s, format!("vocabulary diversity: {diversity:.2}"))
        } else {
            // Generic: combined heuristic.
            let length_factor = (word_count as f32 / 50.0).min(1.0);
            let s = length_factor * 0.7;
            (s, format!("generic heuristic based on {word_count} words"))
        };

        CriterionScore {
            criterion: criterion.to_string(),
            score,
            explanation,
        }
    }

    /// Average number of words per sentence.
    fn avg_sentence_length(text: &str) -> f32 {
        let sentences: Vec<&str> = text
            .split(['.', '!', '?'])
            .filter(|s| !s.trim().is_empty())
            .collect();
        if sentences.is_empty() {
            return 0.0;
        }
        let total_words: usize = sentences
            .iter()
            .map(|s| s.split_whitespace().count())
            .sum();
        total_words as f32 / sentences.len() as f32
    }
}

impl Default for LlmJudge {
    fn default() -> Self {
        Self::new()
    }
}
