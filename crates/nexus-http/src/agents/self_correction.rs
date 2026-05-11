//! Self-Correction — detects stuck loops and injects reflection prompts.
//!
//! During agent execution, the self-correction system monitors tool call
//! patterns and periodically injects reflection prompts asking the agent
//! to evaluate its progress and consider alternative approaches.

use super::events::ToolCallRecord;

/// Configuration for the self-correction system.
#[derive(Debug, Clone)]
pub struct SelfCorrectionConfig {
    /// Inject a reflection prompt every N iterations.
    pub reflection_interval: u32,
    /// Number of recent tool calls to analyze for stuck detection.
    pub stuck_window: usize,
    /// Minimum number of repeated sequences to consider the agent stuck.
    pub stuck_threshold: usize,
}

impl Default for SelfCorrectionConfig {
    fn default() -> Self {
        Self {
            reflection_interval: 3,
            stuck_window: 6,
            stuck_threshold: 2,
        }
    }
}

/// Tracks agent execution state for self-correction analysis.
pub struct SelfCorrectionTracker {
    config: SelfCorrectionConfig,
    tool_history: Vec<String>,
    iteration: u32,
    last_reflection: u32,
}

impl SelfCorrectionTracker {
    /// Create a new tracker with default configuration.
    pub fn new() -> Self {
        Self::with_config(SelfCorrectionConfig::default())
    }

    /// Create a new tracker with custom configuration.
    pub fn with_config(config: SelfCorrectionConfig) -> Self {
        Self {
            config,
            tool_history: Vec::new(),
            iteration: 0,
            last_reflection: 0,
        }
    }

    /// Record a tool call made by the agent.
    pub fn record_tool_call(&mut self, tool_name: &str) {
        self.tool_history.push(tool_name.to_string());
    }

    /// Advance to the next iteration and check if intervention is needed.
    ///
    /// Returns a `CorrectionAction` indicating what the runtime should do.
    pub fn next_iteration(&mut self) -> CorrectionAction {
        self.iteration += 1;

        // Check for stuck loops first (higher priority)
        if self.is_stuck() {
            self.last_reflection = self.iteration;
            return CorrectionAction::InjectUnstuck(self.unstuck_prompt());
        }

        // Check if it's time for a periodic reflection
        if self.iteration - self.last_reflection >= self.config.reflection_interval {
            self.last_reflection = self.iteration;
            return CorrectionAction::InjectReflection(self.reflection_prompt());
        }

        CorrectionAction::Continue
    }

    /// Detect if the agent is stuck in a repeating tool call loop.
    ///
    /// Checks if the last N tool calls form a repeating sequence.
    fn is_stuck(&self) -> bool {
        let window = self.config.stuck_window;
        if self.tool_history.len() < window {
            return false;
        }

        let recent = &self.tool_history[self.tool_history.len() - window..];

        // Check for repeating pairs (A, B, A, B, A, B)
        if window >= 4 {
            let pair_size = 2;
            let pairs_match = recent
                .chunks(pair_size)
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|w| w[0] == w[1])
                .count();
            if pairs_match >= self.config.stuck_threshold {
                return true;
            }
        }

        // Check for repeating triples (A, B, C, A, B, C)
        if window >= 6 {
            let triple_size = 3;
            let triples_match = recent
                .chunks(triple_size)
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|w| w[0] == w[1])
                .count();
            if triples_match >= 1 {
                return true;
            }
        }

        // Check if the same tool is called repeatedly
        let Some(last_tool) = recent.last() else { return false };
        let consecutive_same = recent
            .iter()
            .rev()
            .take_while(|t| *t == last_tool)
            .count();
        consecutive_same >= self.config.stuck_window / 2

    }

    /// Generate a reflection prompt for periodic self-evaluation.
    fn reflection_prompt(&self) -> String {
        format!(
            r#"--- SELF-REFLECTION (iteration {iteration}) ---
Pause and evaluate your progress:
1. What have you accomplished so far?
2. Are you making progress toward the goal?
3. What is the most important next step?
4. Are there any blockers or issues you should address differently?

If you are not making progress, consider:
- Re-reading the original requirements
- Taking a different approach
- Breaking the problem into smaller steps
- Checking for errors in your previous work

Continue with the most impactful next action.
--- END REFLECTION ---"#,
            iteration = self.iteration,
        )
    }

    /// Generate an unstuck prompt when a loop is detected.
    fn unstuck_prompt(&self) -> String {
        let recent_tools: String = self
            .tool_history
            .iter()
            .rev()
            .take(self.config.stuck_window)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(" -> ");

        format!(
            r#"--- STUCK LOOP DETECTED (iteration {iteration}) ---
Your recent tool calls show a repeating pattern: {recent_tools}

This suggests you may be stuck. STOP and reconsider:

1. Why are you repeating the same actions?
2. Is the tool returning unexpected results? If so, read the output carefully.
3. Consider a COMPLETELY DIFFERENT approach:
   - If reading files repeatedly, try searching with grep instead
   - If editing fails, try rewriting the entire file
   - If a command keeps failing, check for prerequisites
   - If the approach is wrong, step back and reconsider the architecture

Do NOT repeat the same tool call sequence. Try something different.
--- END STUCK DETECTION ---"#,
            iteration = self.iteration,
            recent_tools = recent_tools,
        )
    }
}

impl Default for SelfCorrectionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Action the runtime should take based on self-correction analysis.
#[derive(Debug, Clone)]
pub enum CorrectionAction {
    /// No intervention needed; continue normal execution.
    Continue,
    /// Inject a reflection prompt into the conversation.
    InjectReflection(String),
    /// Inject an unstuck prompt (agent is in a loop).
    InjectUnstuck(String),
}

/// Analyze completed tool calls for patterns that suggest problems.
///
/// This is a post-hoc analysis function that can be used for reporting.
pub fn analyze_tool_patterns(calls: &[ToolCallRecord]) -> Vec<String> {
    let mut findings = Vec::new();

    // Check for high failure rate
    let total = calls.len();
    if total > 3 {
        let failures = calls.iter().filter(|c| !c.success).count();
        let rate = failures as f64 / total as f64;
        if rate > 0.5 {
            findings.push(format!(
                "High failure rate: {failures}/{total} tool calls failed ({:.0}%)",
                rate * 100.0
            ));
        }
    }

    // Check for excessive file reads without writes
    let reads = calls.iter().filter(|c| c.tool == "file_read").count();
    let writes = calls
        .iter()
        .filter(|c| c.tool == "file_write" || c.tool == "file_edit")
        .count();
    if reads > 10 && writes == 0 {
        findings.push(format!(
            "Excessive reading without writing: {reads} reads, 0 writes"
        ));
    }

    // Check for repeated identical calls
    if total > 4 {
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for call in calls {
            *seen.entry(format!("{}:{}", call.tool, call.input_summary)).or_default() += 1;
        }
        for (key, count) in &seen {
            if *count >= 3 {
                findings.push(format!("Repeated identical call ({count}x): {key}"));
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_stuck_pair_loop() {
        let mut tracker = SelfCorrectionTracker::new();
        // Simulate A, B, A, B, A, B pattern
        for tool in &["file_read", "file_edit", "file_read", "file_edit", "file_read", "file_edit"] {
            tracker.record_tool_call(tool);
        }
        let action = tracker.next_iteration();
        assert!(matches!(action, CorrectionAction::InjectUnstuck(_)));
    }

    #[test]
    fn reflection_at_interval() {
        let mut tracker = SelfCorrectionTracker::with_config(SelfCorrectionConfig {
            reflection_interval: 2,
            ..Default::default()
        });
        // First iteration: continue
        assert!(matches!(tracker.next_iteration(), CorrectionAction::Continue));
        // Second iteration: reflection
        assert!(matches!(tracker.next_iteration(), CorrectionAction::InjectReflection(_)));
    }

    #[test]
    fn no_false_positive_on_varied_calls() {
        let mut tracker = SelfCorrectionTracker::new();
        for tool in &["file_read", "grep", "file_edit", "bash", "file_read", "glob"] {
            tracker.record_tool_call(tool);
        }
        let action = tracker.next_iteration();
        // Should not be stuck, might be reflection depending on iteration count
        assert!(!matches!(action, CorrectionAction::InjectUnstuck(_)));
    }
}
