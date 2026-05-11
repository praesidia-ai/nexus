//! nexus-quality-core — Quality scoring traits and types for Nexus.
//!
//! This crate defines the public interfaces for:
//! - Quality scoring (the "taste" engine)
//! - Outcome guarantees (multi-cycle repair with certificates)
//! - Fix plans (structured remediation)

pub mod scoring;
pub mod guarantee;
pub mod fix_plan;

pub mod default_scoring;
pub mod default_guarantee;

#[cfg(test)]
mod tests;
