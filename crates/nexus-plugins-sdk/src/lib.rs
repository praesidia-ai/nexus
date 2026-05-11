//! nexus-plugins-sdk — Plugin SDK for Nexus.
//!
//! This crate provides everything an external developer needs to build
//! Nexus plugins: manifest definitions, hook points, capability declarations,
//! and the hook execution interface.

pub mod manifest;
pub mod hooks;
pub mod capabilities;

pub mod default_hook;

#[cfg(test)]
mod tests;
