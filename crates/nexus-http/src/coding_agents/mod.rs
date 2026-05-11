//! Super Coding Agent System — autonomous multi-agent coding pipeline.
//!
//! This module implements a fleet of specialized coding agents that collaborate
//! to understand, plan, implement, review, test, and debug code changes across
//! an entire codebase. It is the evolution of the basic agent loop into a
//! production-grade autonomous coding system.
//!
//! # Architecture
//!
//! ```text
//!                    ┌─────────────────────────────────┐
//!                    │      Coding Orchestrator         │
//!                    │  (coordinates pipeline phases)   │
//!                    └───────────┬─────────────────────┘
//!                                │
//!          ┌─────────────┬───────┴───────┬──────────────┐
//!          ▼             ▼               ▼              ▼
//!    ┌──────────┐  ┌──────────┐   ┌──────────┐   ┌──────────┐
//!    │Architect │  │  Coder   │   │ Reviewer │   │  Tester  │
//!    │(plan)    │  │(implement│   │(validate)│   │(verify)  │
//!    └──────────┘  └──────────┘   └──────────┘   └──────────┘
//!          │             │               │              │
//!          └─────────────┴───────┬───────┴──────────────┘
//!                                ▼
//!                    ┌─────────────────────────────────┐
//!                    │     Shared Coding Workspace      │
//!                    │ (plan, files, decisions, errors) │
//!                    └─────────────────────────────────┘
//!                                │
//!          ┌─────────────────────┴─────────────────────┐
//!          ▼                                           ▼
//!    ┌──────────────┐                           ┌──────────────┐
//!    │  Verification │                           │  Debugger    │
//!    │  (build/test) │◀──────── retry loop ─────│  (fix errors)│
//!    └──────────────┘                           └──────────────┘
//! ```
//!
//! # Integration Points
//!
//! - **ExecutionCore**: All file operations go through validated proposals
//! - **CodeGraph**: Impact analysis before changes, dependency-aware planning
//! - **ProjectBrain**: Persistent memory of project structure and decisions
//! - **ToolRegistry**: file_read, file_write, file_edit, grep, glob, bash, git
//! - **MetricsBus**: Performance metrics fed to Super Agents for optimization
//! - **Evolution Engine**: Learning from successes and failures

pub mod agents;
pub mod engine;
pub mod mini_agent;
pub mod orchestrator;
pub mod swarm;
pub mod traits;
pub mod types;
pub mod verification;
pub mod wave_orchestrator;
pub mod zeroclaw_bridge;
