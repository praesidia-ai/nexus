//! nexus-intelligence — Intelligence traits and types for Nexus.
//!
//! This crate defines the public interfaces for:
//! - Intent analysis (understanding what the user wants)
//! - Architecture decisions (choosing the right stack)
//! - Product intelligence (personas, monetization, copy)
//! - Learning memory (adaptive improvement over time)
//! - Domain packs (vertical knowledge for specific domains)

pub mod intent;
pub mod decisions;
pub mod product;
pub mod learning;
pub mod domain_packs;

pub mod default_intent;
pub mod default_decisions;
pub mod default_product;
pub mod default_learning;

#[cfg(test)]
mod tests;
