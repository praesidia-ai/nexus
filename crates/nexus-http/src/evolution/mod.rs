//! Self-Evolution Engine — Nexus learns from every execution.
//!
//! This module implements a feedback loop that:
//! 1. **Tracks** every coding task execution (metrics, outcomes, patterns)
//! 2. **Learns** from successes and failures (pattern detection)
//! 3. **Optimizes** agent behavior, prompts, and configs automatically
//! 4. **Versions** every improvement for safe rollback
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────────┐     ┌───────────────┐     ┌──────────────────┐
//!  │  Coding Task  │────▶│  Improvement   │────▶│  Pattern Learner │
//!  │  Completes    │     │  Tracker       │     │  (detect trends) │
//!  └──────────────┘     └───────────────┘     └────────┬─────────┘
//!                                                       │
//!                              ┌────────────────────────▼─────────┐
//!                              │     Auto-Optimizer                │
//!                              │  (propose + apply improvements)   │
//!                              └────────────────────────┬─────────┘
//!                                                       │
//!                              ┌────────────────────────▼─────────┐
//!                              │     Evolution State               │
//!                              │  (versioned, rollback-safe)       │
//!                              └──────────────────────────────────┘
//! ```

pub mod optimizer;
pub mod pattern_learner;
pub mod tracker;
pub mod types;
