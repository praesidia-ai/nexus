use std::collections::{HashMap, HashSet, VecDeque};

use chrono::Utc;
use rusqlite::{params, Connection};
use tracing::debug;

use crate::edge::{Edge, Relation};
use crate::entity::{Entity, EntityType};
use crate::error::GraphError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

pub struct GraphStore {
    conn: Connection,
}

impl GraphStore {
    pub fn open(conn: Connection) -> Result<Self, GraphError> {
        let store = Self { conn };
        store.init_tables()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, GraphError> {
        let conn = Connection::open_in_memory()?;
        Self::open(conn)
    }

    fn init_tables(&self) -> Result<(), GraphError> {
        self.conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;

             CREATE TABLE IF NOT EXISTS graph_entities (
                 id          TEXT PRIMARY KEY,
                 project_id  TEXT NOT NULL,
                 name        TEXT NOT NULL,
                 entity_type TEXT NOT NULL,
                 properties  TEXT NOT NULL DEFAULT '{}',
                 created_at  TEXT NOT NULL,
                 updated_at  TEXT NOT NULL,
                 source      TEXT NOT NULL,
                 confidence  REAL NOT NULL DEFAULT 1.0
             );

             CREATE INDEX IF NOT EXISTS idx_ge_project    ON graph_entities(project_id);
             CREATE INDEX IF NOT EXISTS idx_ge_type       ON graph_entities(entity_type);
             CREATE INDEX IF NOT EXISTS idx_ge_name       ON graph_entities(name);

             CREATE TABLE IF NOT EXISTS graph_edges (
                 id          TEXT PRIMARY KEY,
                 source_id   TEXT NOT NULL,
                 target_id   TEXT NOT NULL,
                 relation    TEXT NOT NULL,
                 properties  TEXT NOT NULL DEFAULT '{}',
                 valid_from  TEXT NOT NULL,
                 valid_until TEXT,
                 confidence  REAL NOT NULL DEFAULT 1.0,
                 source      TEXT NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_edges_source   ON graph_edges(source_id);
             CREATE INDEX IF NOT EXISTS idx_edges_target   ON graph_edges(target_id);
             CREATE INDEX IF NOT EXISTS idx_edges_relation ON graph_edges(relation);",
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Entity CRUD
    // -----------------------------------------------------------------------

    pub fn insert_entity(&self, entity: &Entity) -> Result<(), GraphError> {
        let entity_type = serde_json::to_string(&entity.entity_type)?;
        let properties = entity.properties.to_string();
        let created_at = entity.created_at.to_rfc3339();
        let updated_at = entity.updated_at.to_rfc3339();

        self.conn.execute(
            "INSERT OR REPLACE INTO graph_entities
             (id, project_id, name, entity_type, properties, created_at, updated_at, source, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entity.id,
                entity.project_id,
                entity.name,
                entity_type,
                properties,
                created_at,
                updated_at,
                entity.source,
                entity.confidence,
            ],
        )?;
        debug!(entity_id = %entity.id, name = %entity.name, "Inserted entity");
        Ok(())
    }

    pub fn get_entity(&self, id: &str) -> Result<Option<Entity>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, name, entity_type, properties, created_at, updated_at, source, confidence
             FROM graph_entities WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_entity)?;
        match rows.next() {
            Some(Ok(entity)) => Ok(Some(entity)),
            Some(Err(e)) => Err(GraphError::from(e)),
            None => Ok(None),
        }
    }

    pub fn find_entities(
        &self,
        project_id: &str,
        entity_type: Option<&EntityType>,
        name_pattern: Option<&str>,
    ) -> Result<Vec<Entity>, GraphError> {
        let mut conditions = vec!["project_id = ?1".to_string()];
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(project_id.to_string())];

        if let Some(et) = entity_type {
            let et_json = serde_json::to_string(et)?;
            conditions.push(format!("entity_type = ?{}", param_values.len() + 1));
            param_values.push(Box::new(et_json));
        }

        if let Some(pattern) = name_pattern {
            conditions.push(format!("name LIKE ?{}", param_values.len() + 1));
            param_values.push(Box::new(format!("%{pattern}%")));
        }

        let sql = format!(
            "SELECT id, project_id, name, entity_type, properties, created_at, updated_at, source, confidence
             FROM graph_entities WHERE {} ORDER BY updated_at DESC",
            conditions.join(" AND ")
        );

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), row_to_entity)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn delete_entity(&self, id: &str) -> Result<(), GraphError> {
        self.conn
            .execute("DELETE FROM graph_edges WHERE source_id = ?1 OR target_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM graph_entities WHERE id = ?1", params![id])?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Edge CRUD
    // -----------------------------------------------------------------------

    pub fn insert_edge(&self, edge: &Edge) -> Result<(), GraphError> {
        let relation = serde_json::to_string(&edge.relation)?;
        let properties = edge.properties.to_string();
        let valid_from = edge.valid_from.to_rfc3339();
        let valid_until = edge.valid_until.map(|dt| dt.to_rfc3339());

        self.conn.execute(
            "INSERT OR REPLACE INTO graph_edges
             (id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                edge.id,
                edge.source_id,
                edge.target_id,
                relation,
                properties,
                valid_from,
                valid_until,
                edge.confidence,
                edge.source,
            ],
        )?;
        debug!(edge_id = %edge.id, relation = %edge.relation, "Inserted edge");
        Ok(())
    }

    pub fn get_edge(&self, id: &str) -> Result<Option<Edge>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source
             FROM graph_edges WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_edge)?;
        match rows.next() {
            Some(Ok(edge)) => Ok(Some(edge)),
            Some(Err(e)) => Err(GraphError::from(e)),
            None => Ok(None),
        }
    }

    pub fn get_entity_edges(&self, entity_id: &str) -> Result<Vec<Edge>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source
             FROM graph_edges WHERE (source_id = ?1 OR target_id = ?1) AND valid_until IS NULL",
        )?;

        let rows = stmt
            .query_map(params![entity_id], row_to_edge)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn expire_edge(&self, edge_id: &str) -> Result<(), GraphError> {
        let now = Utc::now().to_rfc3339();
        let changed = self.conn.execute(
            "UPDATE graph_edges SET valid_until = ?1 WHERE id = ?2 AND valid_until IS NULL",
            params![now, edge_id],
        )?;
        if changed == 0 {
            return Err(GraphError::NotFound(format!("Edge {edge_id} not found or already expired")));
        }
        debug!(edge_id = %edge_id, "Expired edge");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Neighbor traversal
    // -----------------------------------------------------------------------

    pub fn get_neighbors(
        &self,
        entity_id: &str,
        relation: Option<&Relation>,
        direction: Direction,
    ) -> Result<Vec<(Edge, Entity)>, GraphError> {
        let mut results = Vec::new();

        if direction == Direction::Outgoing || direction == Direction::Both {
            let edges = self.outgoing_edges(entity_id, relation)?;
            for edge in edges {
                if let Some(target) = self.get_entity(&edge.target_id)? {
                    results.push((edge, target));
                }
            }
        }

        if direction == Direction::Incoming || direction == Direction::Both {
            let edges = self.incoming_edges(entity_id, relation)?;
            for edge in edges {
                if let Some(source) = self.get_entity(&edge.source_id)? {
                    results.push((edge, source));
                }
            }
        }

        Ok(results)
    }

    fn outgoing_edges(
        &self,
        entity_id: &str,
        relation: Option<&Relation>,
    ) -> Result<Vec<Edge>, GraphError> {
        if let Some(rel) = relation {
            let rel_json = serde_json::to_string(rel)?;
            let mut stmt = self.conn.prepare(
                "SELECT id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source
                 FROM graph_edges WHERE source_id = ?1 AND relation = ?2 AND valid_until IS NULL",
            )?;
            let rows = stmt
                .query_map(params![entity_id, rel_json], row_to_edge)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source
                 FROM graph_edges WHERE source_id = ?1 AND valid_until IS NULL",
            )?;
            let rows = stmt
                .query_map(params![entity_id], row_to_edge)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
    }

    fn incoming_edges(
        &self,
        entity_id: &str,
        relation: Option<&Relation>,
    ) -> Result<Vec<Edge>, GraphError> {
        if let Some(rel) = relation {
            let rel_json = serde_json::to_string(rel)?;
            let mut stmt = self.conn.prepare(
                "SELECT id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source
                 FROM graph_edges WHERE target_id = ?1 AND relation = ?2 AND valid_until IS NULL",
            )?;
            let rows = stmt
                .query_map(params![entity_id, rel_json], row_to_edge)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source
                 FROM graph_edges WHERE target_id = ?1 AND valid_until IS NULL",
            )?;
            let rows = stmt
                .query_map(params![entity_id], row_to_edge)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
    }

    // -----------------------------------------------------------------------
    // BFS path finding
    // -----------------------------------------------------------------------

    pub fn find_path(
        &self,
        from_id: &str,
        to_id: &str,
        max_depth: usize,
    ) -> Result<Option<Vec<(Edge, Entity)>>, GraphError> {
        if from_id == to_id {
            return Ok(Some(Vec::new()));
        }

        let mut visited: HashSet<String> = HashSet::new();
        // parent map: child_entity_id -> (edge, parent_entity_id)
        let mut parent: HashMap<String, (Edge, String)> = HashMap::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        visited.insert(from_id.to_string());
        queue.push_back((from_id.to_string(), 0));

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let edges = self.get_entity_edges(&current_id)?;
            for edge in edges {
                let neighbor_id = if edge.source_id == current_id {
                    edge.target_id.clone()
                } else {
                    edge.source_id.clone()
                };

                if visited.contains(neighbor_id.as_str()) {
                    continue;
                }

                visited.insert(neighbor_id.clone());
                let found = neighbor_id == to_id;
                parent.insert(neighbor_id.clone(), (edge, current_id.clone()));

                if found {
                    return Ok(Some(self.reconstruct_path(&parent, from_id, to_id)?));
                }

                queue.push_back((neighbor_id, depth + 1));
            }
        }

        Ok(None)
    }

    fn reconstruct_path(
        &self,
        parent: &HashMap<String, (Edge, String)>,
        from_id: &str,
        to_id: &str,
    ) -> Result<Vec<(Edge, Entity)>, GraphError> {
        let mut path = Vec::new();
        let mut current = to_id.to_string();

        while current != from_id {
            let (edge, prev) = parent
                .get(&current)
                .ok_or_else(|| GraphError::NotFound("Path reconstruction failed".to_string()))?;

            if let Some(entity) = self.get_entity(&current)? {
                path.push((edge.clone(), entity));
            }
            current = prev.clone();
        }

        path.reverse();
        Ok(path)
    }

    // -----------------------------------------------------------------------
    // Aggregate queries
    // -----------------------------------------------------------------------

    pub fn entity_count(&self, project_id: &str) -> Result<usize, GraphError> {
        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM graph_entities WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn edge_count(&self, project_id: Option<&str>) -> Result<usize, GraphError> {
        if let Some(pid) = project_id {
            let count: usize = self.conn.query_row(
                "SELECT COUNT(*) FROM graph_edges e
                 JOIN graph_entities src ON e.source_id = src.id
                 WHERE src.project_id = ?1 AND e.valid_until IS NULL",
                params![pid],
                |row| row.get(0),
            )?;
            Ok(count)
        } else {
            let count: usize = self.conn.query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE valid_until IS NULL",
                [],
                |row| row.get(0),
            )?;
            Ok(count)
        }
    }

    pub fn all_active_edges(&self) -> Result<Vec<Edge>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, target_id, relation, properties, valid_from, valid_until, confidence, source
             FROM graph_edges WHERE valid_until IS NULL",
        )?;
        let rows = stmt
            .query_map([], row_to_edge)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn all_entities(&self, project_id: &str) -> Result<Vec<Entity>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, name, entity_type, properties, created_at, updated_at, source, confidence
             FROM graph_entities WHERE project_id = ?1 ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map(params![project_id], row_to_entity)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn entity_edge_counts(&self, project_id: &str) -> Result<HashMap<String, usize>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, (
                SELECT COUNT(*) FROM graph_edges ge
                WHERE (ge.source_id = e.id OR ge.target_id = e.id) AND ge.valid_until IS NULL
             ) as edge_count
             FROM graph_entities e WHERE e.project_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![project_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows.into_iter().collect())
    }

    pub fn find_duplicate_entities(&self, project_id: &str) -> Result<Vec<Vec<Entity>>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, entity_type, COUNT(*) as cnt
             FROM graph_entities WHERE project_id = ?1
             GROUP BY name, entity_type HAVING cnt > 1",
        )?;

        let dupes: Vec<(String, String)> = stmt
            .query_map(params![project_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut groups = Vec::new();
        for (name, entity_type) in dupes {
            let mut stmt2 = self.conn.prepare(
                "SELECT id, project_id, name, entity_type, properties, created_at, updated_at, source, confidence
                 FROM graph_entities WHERE project_id = ?1 AND name = ?2 AND entity_type = ?3
                 ORDER BY updated_at DESC",
            )?;
            let entities: Vec<Entity> = stmt2
                .query_map(params![project_id, name, entity_type], row_to_entity)?
                .collect::<Result<Vec<_>, _>>()?;
            groups.push(entities);
        }

        Ok(groups)
    }

    pub fn merge_entities(&self, keep_id: &str, remove_id: &str) -> Result<(), GraphError> {
        self.conn.execute(
            "UPDATE graph_edges SET source_id = ?1 WHERE source_id = ?2",
            params![keep_id, remove_id],
        )?;
        self.conn.execute(
            "UPDATE graph_edges SET target_id = ?1 WHERE target_id = ?2",
            params![keep_id, remove_id],
        )?;
        self.conn.execute(
            "DELETE FROM graph_entities WHERE id = ?1",
            params![remove_id],
        )?;
        debug!(keep = %keep_id, remove = %remove_id, "Merged entities");
        Ok(())
    }

    pub fn expire_stale_edges(&self, days: i64) -> Result<usize, GraphError> {
        let cutoff = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let now = Utc::now().to_rfc3339();
        let changed = self.conn.execute(
            "UPDATE graph_edges SET valid_until = ?1
             WHERE valid_until IS NULL AND valid_from < ?2",
            params![now, cutoff],
        )?;
        debug!(expired = changed, days = days, "Expired stale edges");
        Ok(changed)
    }
}

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

fn row_to_entity(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entity> {
    let entity_type_str: String = row.get(3)?;
    let properties_str: String = row.get(4)?;
    let created_at_str: String = row.get(5)?;
    let updated_at_str: String = row.get(6)?;

    let entity_type: EntityType =
        serde_json::from_str(&entity_type_str).unwrap_or(EntityType::Custom("unknown".to_string()));
    let properties: serde_json::Value =
        serde_json::from_str(&properties_str).unwrap_or(serde_json::Value::Object(Default::default()));
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(Entity {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        entity_type,
        properties,
        created_at,
        updated_at,
        source: row.get(7)?,
        confidence: row.get(8)?,
    })
}

fn row_to_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<Edge> {
    let relation_str: String = row.get(3)?;
    let properties_str: String = row.get(4)?;
    let valid_from_str: String = row.get(5)?;
    let valid_until_str: Option<String> = row.get(6)?;

    let relation: Relation =
        serde_json::from_str(&relation_str).unwrap_or(Relation::Custom("unknown".to_string()));
    let properties: serde_json::Value =
        serde_json::from_str(&properties_str).unwrap_or(serde_json::Value::Object(Default::default()));
    let valid_from = chrono::DateTime::parse_from_rfc3339(&valid_from_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let valid_until = valid_until_str.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .ok()
    });

    Ok(Edge {
        id: row.get(0)?,
        source_id: row.get(1)?,
        target_id: row.get(2)?,
        relation,
        properties,
        valid_from,
        valid_until,
        confidence: row.get(7)?,
        source: row.get(8)?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::Relation;
    use crate::entity::EntityType;

    fn test_store() -> GraphStore {
        GraphStore::open_in_memory().unwrap()
    }

    fn make_entity(id: &str, name: &str, entity_type: EntityType) -> Entity {
        Entity {
            id: id.to_string(),
            project_id: "proj1".to_string(),
            name: name.to_string(),
            entity_type,
            properties: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source: "test".to_string(),
            confidence: 1.0,
        }
    }

    fn make_edge(id: &str, source: &str, target: &str, relation: Relation) -> Edge {
        Edge {
            id: id.to_string(),
            source_id: source.to_string(),
            target_id: target.to_string(),
            relation,
            properties: serde_json::json!({}),
            valid_from: Utc::now(),
            valid_until: None,
            confidence: 1.0,
            source: "test".to_string(),
        }
    }

    #[test]
    fn insert_and_get_entity() {
        let store = test_store();
        let entity = make_entity("e1", "main.rs", EntityType::File);
        store.insert_entity(&entity).unwrap();

        let fetched = store.get_entity("e1").unwrap().unwrap();
        assert_eq!(fetched.name, "main.rs");
        assert_eq!(fetched.entity_type, EntityType::File);
    }

    #[test]
    fn get_entity_not_found() {
        let store = test_store();
        assert!(store.get_entity("nonexistent").unwrap().is_none());
    }

    #[test]
    fn find_entities_by_type() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "main.rs", EntityType::File)).unwrap();
        store.insert_entity(&make_entity("e2", "parse", EntityType::Function)).unwrap();
        store.insert_entity(&make_entity("e3", "lib.rs", EntityType::File)).unwrap();

        let files = store.find_entities("proj1", Some(&EntityType::File), None).unwrap();
        assert_eq!(files.len(), 2);

        let fns = store.find_entities("proj1", Some(&EntityType::Function), None).unwrap();
        assert_eq!(fns.len(), 1);
    }

    #[test]
    fn find_entities_by_name_pattern() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "main.rs", EntityType::File)).unwrap();
        store.insert_entity(&make_entity("e2", "lib.rs", EntityType::File)).unwrap();
        store.insert_entity(&make_entity("e3", "utils.ts", EntityType::File)).unwrap();

        let results = store.find_entities("proj1", None, Some(".rs")).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn insert_and_get_edge() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "main.rs", EntityType::File)).unwrap();
        store.insert_entity(&make_entity("e2", "lib.rs", EntityType::File)).unwrap();

        let edge = make_edge("edge1", "e1", "e2", Relation::Imports);
        store.insert_edge(&edge).unwrap();

        let fetched = store.get_edge("edge1").unwrap().unwrap();
        assert_eq!(fetched.source_id, "e1");
        assert_eq!(fetched.target_id, "e2");
        assert_eq!(fetched.relation, Relation::Imports);
    }

    #[test]
    fn get_entity_edges() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e3", "C", EntityType::Module)).unwrap();

        store.insert_edge(&make_edge("ed1", "e1", "e2", Relation::Imports)).unwrap();
        store.insert_edge(&make_edge("ed2", "e3", "e1", Relation::DependsOn)).unwrap();

        let edges = store.get_entity_edges("e1").unwrap();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn expire_edge() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        store.insert_edge(&make_edge("ed1", "e1", "e2", Relation::Imports)).unwrap();

        store.expire_edge("ed1").unwrap();

        let edge = store.get_edge("ed1").unwrap().unwrap();
        assert!(edge.valid_until.is_some());

        let active = store.get_entity_edges("e1").unwrap();
        assert!(active.is_empty());
    }

    #[test]
    fn expire_edge_not_found() {
        let store = test_store();
        assert!(store.expire_edge("nonexistent").is_err());
    }

    #[test]
    fn get_neighbors_outgoing() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e3", "C", EntityType::Module)).unwrap();

        store.insert_edge(&make_edge("ed1", "e1", "e2", Relation::Imports)).unwrap();
        store.insert_edge(&make_edge("ed2", "e1", "e3", Relation::Calls)).unwrap();

        let neighbors = store.get_neighbors("e1", None, Direction::Outgoing).unwrap();
        assert_eq!(neighbors.len(), 2);

        let import_neighbors = store.get_neighbors("e1", Some(&Relation::Imports), Direction::Outgoing).unwrap();
        assert_eq!(import_neighbors.len(), 1);
        assert_eq!(import_neighbors[0].1.name, "B");
    }

    #[test]
    fn find_path_direct() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        store.insert_edge(&make_edge("ed1", "e1", "e2", Relation::Imports)).unwrap();

        let path = store.find_path("e1", "e2", 5).unwrap().unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].1.name, "B");
    }

    #[test]
    fn find_path_multi_hop() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e3", "C", EntityType::Module)).unwrap();

        store.insert_edge(&make_edge("ed1", "e1", "e2", Relation::Imports)).unwrap();
        store.insert_edge(&make_edge("ed2", "e2", "e3", Relation::Calls)).unwrap();

        let path = store.find_path("e1", "e3", 5).unwrap().unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].1.name, "B");
        assert_eq!(path[1].1.name, "C");
    }

    #[test]
    fn find_path_no_path() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();

        let path = store.find_path("e1", "e2", 5).unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn find_path_same_node() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        let path = store.find_path("e1", "e1", 5).unwrap().unwrap();
        assert!(path.is_empty());
    }

    #[test]
    fn find_path_depth_limited() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e3", "C", EntityType::Module)).unwrap();

        store.insert_edge(&make_edge("ed1", "e1", "e2", Relation::Imports)).unwrap();
        store.insert_edge(&make_edge("ed2", "e2", "e3", Relation::Calls)).unwrap();

        let path = store.find_path("e1", "e3", 1).unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn delete_entity_cascades_edges() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        store.insert_edge(&make_edge("ed1", "e1", "e2", Relation::Imports)).unwrap();

        store.delete_entity("e1").unwrap();
        assert!(store.get_entity("e1").unwrap().is_none());
        assert!(store.get_edge("ed1").unwrap().is_none());
    }

    #[test]
    fn entity_count() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "B", EntityType::Module)).unwrap();
        assert_eq!(store.entity_count("proj1").unwrap(), 2);
        assert_eq!(store.entity_count("proj2").unwrap(), 0);
    }

    #[test]
    fn merge_entities_repoints_edges() {
        let store = test_store();
        store.insert_entity(&make_entity("e1", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e2", "A", EntityType::Module)).unwrap();
        store.insert_entity(&make_entity("e3", "B", EntityType::Module)).unwrap();

        store.insert_edge(&make_edge("ed1", "e2", "e3", Relation::Calls)).unwrap();

        store.merge_entities("e1", "e2").unwrap();

        assert!(store.get_entity("e2").unwrap().is_none());
        let edge = store.get_edge("ed1").unwrap().unwrap();
        assert_eq!(edge.source_id, "e1");
    }
}
