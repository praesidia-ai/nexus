pub mod compression;
pub mod engine;
pub mod error;
pub mod layer1;
pub mod layer2;
pub mod layer3;

pub use compression::{CompressionLevel, CompressionResult};
pub use engine::ContextEngine;
pub use error::ContextError;
pub use layer1::{Decision, DirectoryEntry, ProjectIndex};
pub use layer2::{ContextItem, ContextKind, ContextPriority, SessionMemory};
pub use layer3::{FactCategory, KnowledgeFact, PersistentKnowledge};
