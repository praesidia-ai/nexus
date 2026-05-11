//! nexus-eval — Evaluation and benchmarking framework for Nexus.
//!
//! Provides evaluation suites, a test runner, LLM-as-judge scoring,
//! and regression detection to ensure quality never silently degrades.

pub mod anthropic_judge;
pub mod error;
pub mod judge;
pub mod regression;
pub mod runner;
pub mod suite;
pub mod token_bench;

#[cfg(test)]
mod tests;

pub use error::EvalError;
