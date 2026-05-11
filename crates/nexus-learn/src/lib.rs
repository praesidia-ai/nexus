//! nexus-learn — Self-improving agent intelligence.
//!
//! Provides outcome tracking, pattern extraction from historical data,
//! and eval-gated skill promotion so agents get better over time.

pub mod error;
pub mod eval;
pub mod outcome;
pub mod pattern;

#[cfg(test)]
mod tests;

pub use error::LearnError;
