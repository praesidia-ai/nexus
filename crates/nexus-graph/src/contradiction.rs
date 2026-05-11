use serde::{Deserialize, Serialize};

use crate::edge::{Edge, Relation};
use crate::error::GraphError;
use crate::store::GraphStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub edge_a: Edge,
    pub edge_b: Edge,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionStrategy {
    KeepNewer,
    KeepHigherConfidence,
    KeepBoth,
    ExpireBoth,
}

pub fn detect_contradictions(store: &GraphStore, _project_id: &str) -> Result<Vec<Contradiction>, GraphError> {
    let edges = store.all_active_edges()?;
    let mut contradictions = Vec::new();

    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            let a = &edges[i];
            let b = &edges[j];

            if let Some(reason) = check_contradiction(a, b) {
                contradictions.push(Contradiction {
                    edge_a: a.clone(),
                    edge_b: b.clone(),
                    reason,
                });
            }
        }
    }

    Ok(contradictions)
}

fn check_contradiction(a: &Edge, b: &Edge) -> Option<String> {
    // Same source and target with conflicting relations
    if a.source_id == b.source_id && a.target_id == b.target_id {
        if a.relation == Relation::ConflictsWith || b.relation == Relation::ConflictsWith {
            return Some(format!(
                "Explicit conflict between edges {} and {}",
                a.id, b.id
            ));
        }

        if a.relation.is_conflict_pair(&b.relation) {
            return Some(format!(
                "Contradicting relations: {} vs {} between same entities",
                a.relation, b.relation
            ));
        }

        if a.relation == Relation::Supersedes && b.relation == a.relation {
            return Some(format!(
                "Duplicate supersedes edges {} and {}",
                a.id, b.id
            ));
        }
    }

    // A supersedes B's target, but B still has an active edge to it
    if a.relation == Relation::Supersedes
        && a.target_id == b.target_id
        && a.source_id != b.source_id
        && b.relation == Relation::DependsOn
    {
        return Some(format!(
            "Entity {} superseded but {} still depends on it",
            a.target_id, b.source_id
        ));
    }

    // Circular dependency: A depends_on B and B depends_on A
    if a.relation == Relation::DependsOn
        && b.relation == Relation::DependsOn
        && a.source_id == b.target_id
        && a.target_id == b.source_id
    {
        return Some(format!(
            "Circular dependency between {} and {}",
            a.source_id, a.target_id
        ));
    }

    None
}

pub fn resolve_contradiction(
    store: &GraphStore,
    contradiction: &Contradiction,
    strategy: ResolutionStrategy,
) -> Result<(), GraphError> {
    match strategy {
        ResolutionStrategy::KeepNewer => {
            if contradiction.edge_a.valid_from >= contradiction.edge_b.valid_from {
                store.expire_edge(&contradiction.edge_b.id)?;
            } else {
                store.expire_edge(&contradiction.edge_a.id)?;
            }
        }
        ResolutionStrategy::KeepHigherConfidence => {
            if contradiction.edge_a.confidence >= contradiction.edge_b.confidence {
                store.expire_edge(&contradiction.edge_b.id)?;
            } else {
                store.expire_edge(&contradiction.edge_a.id)?;
            }
        }
        ResolutionStrategy::KeepBoth => {
            // no-op
        }
        ResolutionStrategy::ExpireBoth => {
            store.expire_edge(&contradiction.edge_a.id)?;
            store.expire_edge(&contradiction.edge_b.id)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::Relation;
    use crate::entity::EntityType;
    use crate::store::GraphStore;
    use chrono::Utc;

    fn setup() -> GraphStore {
        let store = GraphStore::open_in_memory().unwrap();
        let now = Utc::now();
        store
            .insert_entity(&crate::entity::Entity {
                id: "e1".to_string(),
                project_id: "proj1".to_string(),
                name: "A".to_string(),
                entity_type: EntityType::Module,
                properties: serde_json::json!({}),
                created_at: now,
                updated_at: now,
                source: "test".to_string(),
                confidence: 1.0,
            })
            .unwrap();
        store
            .insert_entity(&crate::entity::Entity {
                id: "e2".to_string(),
                project_id: "proj1".to_string(),
                name: "B".to_string(),
                entity_type: EntityType::Module,
                properties: serde_json::json!({}),
                created_at: now,
                updated_at: now,
                source: "test".to_string(),
                confidence: 1.0,
            })
            .unwrap();
        store
    }

    #[test]
    fn detect_circular_dependency() {
        let store = setup();
        let now = Utc::now();

        store
            .insert_edge(&Edge {
                id: "ed1".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        store
            .insert_edge(&Edge {
                id: "ed2".to_string(),
                source_id: "e2".to_string(),
                target_id: "e1".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        let contradictions = detect_contradictions(&store, "proj1").unwrap();
        assert_eq!(contradictions.len(), 1);
        assert!(contradictions[0].reason.contains("Circular dependency"));
    }

    #[test]
    fn detect_conflicting_relations() {
        let store = setup();
        let now = Utc::now();

        store
            .insert_edge(&Edge {
                id: "ed1".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        store
            .insert_edge(&Edge {
                id: "ed2".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::BlockedBy,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        let contradictions = detect_contradictions(&store, "proj1").unwrap();
        assert_eq!(contradictions.len(), 1);
        assert!(contradictions[0].reason.contains("Contradicting relations"));
    }

    #[test]
    fn no_contradictions_when_clean() {
        let store = setup();
        let now = Utc::now();

        store
            .insert_edge(&Edge {
                id: "ed1".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::Imports,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        let contradictions = detect_contradictions(&store, "proj1").unwrap();
        assert!(contradictions.is_empty());
    }

    #[test]
    fn resolve_keep_newer() {
        let store = setup();
        let t1 = Utc::now() - chrono::Duration::hours(2);
        let t2 = Utc::now();

        store
            .insert_edge(&Edge {
                id: "ed1".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: t1,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        store
            .insert_edge(&Edge {
                id: "ed2".to_string(),
                source_id: "e2".to_string(),
                target_id: "e1".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: t2,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        let contradictions = detect_contradictions(&store, "proj1").unwrap();
        assert_eq!(contradictions.len(), 1);

        resolve_contradiction(&store, &contradictions[0], ResolutionStrategy::KeepNewer).unwrap();

        let ed1 = store.get_edge("ed1").unwrap().unwrap();
        let ed2 = store.get_edge("ed2").unwrap().unwrap();
        assert!(ed1.valid_until.is_some()); // older one expired
        assert!(ed2.valid_until.is_none()); // newer one kept
    }

    #[test]
    fn resolve_keep_higher_confidence() {
        let store = setup();
        let now = Utc::now();

        store
            .insert_edge(&Edge {
                id: "ed1".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 0.9,
                source: "test".to_string(),
            })
            .unwrap();

        store
            .insert_edge(&Edge {
                id: "ed2".to_string(),
                source_id: "e2".to_string(),
                target_id: "e1".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 0.5,
                source: "test".to_string(),
            })
            .unwrap();

        let contradictions = detect_contradictions(&store, "proj1").unwrap();
        resolve_contradiction(&store, &contradictions[0], ResolutionStrategy::KeepHigherConfidence).unwrap();

        let ed1 = store.get_edge("ed1").unwrap().unwrap();
        let ed2 = store.get_edge("ed2").unwrap().unwrap();
        assert!(ed1.valid_until.is_none()); // higher confidence kept
        assert!(ed2.valid_until.is_some()); // lower confidence expired
    }

    #[test]
    fn resolve_expire_both() {
        let store = setup();
        let now = Utc::now();

        store
            .insert_edge(&Edge {
                id: "ed1".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        store
            .insert_edge(&Edge {
                id: "ed2".to_string(),
                source_id: "e2".to_string(),
                target_id: "e1".to_string(),
                relation: Relation::DependsOn,
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        let contradictions = detect_contradictions(&store, "proj1").unwrap();
        resolve_contradiction(&store, &contradictions[0], ResolutionStrategy::ExpireBoth).unwrap();

        let ed1 = store.get_edge("ed1").unwrap().unwrap();
        let ed2 = store.get_edge("ed2").unwrap().unwrap();
        assert!(ed1.valid_until.is_some());
        assert!(ed2.valid_until.is_some());
    }
}
