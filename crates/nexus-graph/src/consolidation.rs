use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::contradiction::{detect_contradictions, Contradiction};
use crate::error::GraphError;
use crate::store::GraphStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    pub entities_before: usize,
    pub entities_after: usize,
    pub duplicates_merged: usize,
    pub edges_expired: usize,
    pub contradictions_found: Vec<Contradiction>,
    pub importance_scores: Vec<ImportanceScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceScore {
    pub entity_id: String,
    pub entity_name: String,
    pub score: f64,
    pub edge_count: usize,
}

pub fn consolidate(store: &GraphStore, project_id: &str) -> Result<ConsolidationResult, GraphError> {
    let entities_before = store.entity_count(project_id)?;
    info!(project_id = %project_id, entities = entities_before, "Starting consolidation");

    // 1. Merge duplicate entities (same name + type)
    let duplicates_merged = merge_duplicates(store, project_id)?;
    debug!(merged = duplicates_merged, "Merged duplicate entities");

    // 2. Expire stale edges (not updated in 30 days)
    let edges_expired = store.expire_stale_edges(30)?;
    debug!(expired = edges_expired, "Expired stale edges");

    // 3. Detect and flag contradictions
    let contradictions_found = detect_contradictions(store, project_id)?;
    debug!(count = contradictions_found.len(), "Detected contradictions");

    // 4. Compute entity importance scores (based on edge count)
    let importance_scores = compute_importance(store, project_id)?;

    let entities_after = store.entity_count(project_id)?;

    let result = ConsolidationResult {
        entities_before,
        entities_after,
        duplicates_merged,
        edges_expired,
        contradictions_found,
        importance_scores,
    };

    info!(
        project_id = %project_id,
        before = result.entities_before,
        after = result.entities_after,
        merged = result.duplicates_merged,
        expired = result.edges_expired,
        contradictions = result.contradictions_found.len(),
        "Consolidation complete"
    );

    Ok(result)
}

fn merge_duplicates(store: &GraphStore, project_id: &str) -> Result<usize, GraphError> {
    let groups = store.find_duplicate_entities(project_id)?;
    let mut total_merged = 0;

    for group in groups {
        if group.len() < 2 {
            continue;
        }

        // Keep the one with the highest confidence, or the most recently updated
        let keep = group
            .iter()
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.updated_at.cmp(&b.updated_at))
            })
            .unwrap();

        for entity in &group {
            if entity.id != keep.id {
                store.merge_entities(&keep.id, &entity.id)?;
                total_merged += 1;
            }
        }
    }

    Ok(total_merged)
}

fn compute_importance(store: &GraphStore, project_id: &str) -> Result<Vec<ImportanceScore>, GraphError> {
    let edge_counts = store.entity_edge_counts(project_id)?;
    let entities = store.all_entities(project_id)?;

    let max_edges = edge_counts.values().max().copied().unwrap_or(1).max(1) as f64;

    let mut scores: Vec<ImportanceScore> = entities
        .iter()
        .map(|e| {
            let count = edge_counts.get(&e.id).copied().unwrap_or(0);
            let score = count as f64 / max_edges;
            ImportanceScore {
                entity_id: e.id.clone(),
                entity_name: e.name.clone(),
                score,
                edge_count: count,
            }
        })
        .collect();

    scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scores)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::{Edge, Relation};
    use crate::entity::{Entity, EntityType};
    use crate::store::GraphStore;
    use chrono::Utc;

    fn make_entity(id: &str, name: &str) -> Entity {
        Entity {
            id: id.to_string(),
            project_id: "proj1".to_string(),
            name: name.to_string(),
            entity_type: EntityType::Module,
            properties: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source: "test".to_string(),
            confidence: 1.0,
        }
    }

    fn make_edge(id: &str, source: &str, target: &str) -> Edge {
        Edge {
            id: id.to_string(),
            source_id: source.to_string(),
            target_id: target.to_string(),
            relation: Relation::Imports,
            properties: serde_json::json!({}),
            valid_from: Utc::now(),
            valid_until: None,
            confidence: 1.0,
            source: "test".to_string(),
        }
    }

    #[test]
    fn consolidation_empty_graph() {
        let store = GraphStore::open_in_memory().unwrap();
        let result = consolidate(&store, "proj1").unwrap();
        assert_eq!(result.entities_before, 0);
        assert_eq!(result.entities_after, 0);
        assert_eq!(result.duplicates_merged, 0);
        assert_eq!(result.edges_expired, 0);
        assert!(result.contradictions_found.is_empty());
        assert!(result.importance_scores.is_empty());
    }

    #[test]
    fn consolidation_merges_duplicates() {
        let store = GraphStore::open_in_memory().unwrap();
        // Two entities with same name and type
        let mut e1 = make_entity("e1", "AuthModule");
        e1.confidence = 0.9;
        let mut e2 = make_entity("e2", "AuthModule");
        e2.confidence = 0.5;
        store.insert_entity(&e1).unwrap();
        store.insert_entity(&e2).unwrap();

        // Edge attached to the one that will be removed
        store.insert_edge(&make_edge("ed1", "e2", "e1")).unwrap();
        // Another entity for edge target
        store.insert_entity(&make_entity("e3", "UserModule")).unwrap();
        store.insert_edge(&make_edge("ed2", "e2", "e3")).unwrap();

        let result = consolidate(&store, "proj1").unwrap();
        assert_eq!(result.duplicates_merged, 1);
        assert_eq!(result.entities_after, 2); // e1 (kept) + e3

        // The edge should now point from e1 (the kept entity)
        let edge = store.get_edge("ed2").unwrap().unwrap();
        assert_eq!(edge.source_id, "e1");
    }

    #[test]
    fn consolidation_computes_importance() {
        let store = GraphStore::open_in_memory().unwrap();
        store.insert_entity(&make_entity("e1", "CoreModule")).unwrap();
        store.insert_entity(&make_entity("e2", "HelperModule")).unwrap();
        store.insert_entity(&make_entity("e3", "UtilModule")).unwrap();

        // e1 has 2 edges, e2 has 1, e3 has 0
        store.insert_edge(&make_edge("ed1", "e1", "e2")).unwrap();
        store.insert_edge(&make_edge("ed2", "e3", "e1")).unwrap();

        let result = consolidate(&store, "proj1").unwrap();

        let core_score = result
            .importance_scores
            .iter()
            .find(|s| s.entity_name == "CoreModule")
            .unwrap();
        assert_eq!(core_score.edge_count, 2);
        assert!((core_score.score - 1.0).abs() < f64::EPSILON);

        let helper_score = result
            .importance_scores
            .iter()
            .find(|s| s.entity_name == "HelperModule")
            .unwrap();
        assert_eq!(helper_score.edge_count, 1);
        assert!((helper_score.score - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn consolidation_detects_contradictions() {
        let store = GraphStore::open_in_memory().unwrap();
        store.insert_entity(&make_entity("e1", "A")).unwrap();
        store.insert_entity(&make_entity("e2", "B")).unwrap();

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

        let result = consolidate(&store, "proj1").unwrap();
        assert_eq!(result.contradictions_found.len(), 1);
    }

    #[test]
    fn consolidation_expires_stale_edges() {
        let store = GraphStore::open_in_memory().unwrap();
        store.insert_entity(&make_entity("e1", "A")).unwrap();
        store.insert_entity(&make_entity("e2", "B")).unwrap();

        let old_time = Utc::now() - chrono::Duration::days(60);
        store
            .insert_edge(&Edge {
                id: "ed1".to_string(),
                source_id: "e1".to_string(),
                target_id: "e2".to_string(),
                relation: Relation::Imports,
                properties: serde_json::json!({}),
                valid_from: old_time,
                valid_until: None,
                confidence: 1.0,
                source: "test".to_string(),
            })
            .unwrap();

        let result = consolidate(&store, "proj1").unwrap();
        assert_eq!(result.edges_expired, 1);
    }
}
