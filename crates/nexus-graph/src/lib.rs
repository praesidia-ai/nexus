pub mod consolidation;
pub mod contradiction;
pub mod edge;
pub mod entity;
pub mod error;
pub mod extract;
pub mod store;

pub use consolidation::{consolidate, ConsolidationResult, ImportanceScore};
pub use contradiction::{
    detect_contradictions, resolve_contradiction, Contradiction, ResolutionStrategy,
};
pub use edge::{Edge, Relation};
pub use entity::{Entity, EntityType};
pub use error::GraphError;
pub use extract::{extract_entities, extract_relations};
pub use store::{Direction, GraphStore};
