//! `nexus-sdk-client` — Rust SDK for embedding Nexus agents in applications.
//!
//! Provides a typed HTTP client that wraps the Nexus REST API for generation,
//! agent execution, team orchestration, quality scoring, intent analysis, and
//! memory operations.
//!
//! # Quick Start
//! ```rust,ignore
//! use nexus_sdk_client::NexusClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), nexus_sdk_client::SdkError> {
//!     let client = NexusClient::new("http://localhost:8020", "your-api-key");
//!     let result = client.generate("Build a todo app").await?;
//!     println!("Project {} created with taste score {}", result.project_id, result.taste_score);
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod error;
pub mod types;

pub use client::NexusClient;
pub use error::SdkError;
