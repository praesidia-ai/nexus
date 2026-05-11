use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeFact {
    pub id: String,
    pub project_id: String,
    pub category: FactCategory,
    pub content: String,
    pub confidence: f64,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub last_verified: DateTime<Utc>,
    pub access_count: u32,
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FactCategory {
    Architecture,
    Convention,
    Decision,
    Pattern,
    Preference,
    Constraint,
    Learned,
}

impl KnowledgeFact {
    pub fn category_label(&self) -> &'static str {
        match self.category {
            FactCategory::Architecture => "arch",
            FactCategory::Convention => "conv",
            FactCategory::Decision => "decision",
            FactCategory::Pattern => "pattern",
            FactCategory::Preference => "pref",
            FactCategory::Constraint => "constraint",
            FactCategory::Learned => "learned",
        }
    }
}

/// L3 — Persistent Knowledge Store backed by SQLite.
pub struct PersistentKnowledge {
    db: Connection,
}

impl PersistentKnowledge {
    pub fn new(db_path: &std::path::Path) -> Result<Self, rusqlite::Error> {
        let db = Connection::open(db_path)?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_facts (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                category TEXT NOT NULL,
                content TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 1.0,
                source TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                last_verified TEXT NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                superseded_by TEXT,
                content_hash TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_kf_project ON knowledge_facts(project_id);
            CREATE INDEX IF NOT EXISTS idx_kf_category ON knowledge_facts(project_id, category);
            CREATE INDEX IF NOT EXISTS idx_kf_hash ON knowledge_facts(content_hash);",
        )?;
        Ok(Self { db })
    }

    pub fn store(&self, fact: &KnowledgeFact) -> Result<(), rusqlite::Error> {
        let hash = content_hash(&fact.content);

        if let Some(existing_id) = self.find_by_hash(&hash)? {
            self.db.execute(
                "UPDATE knowledge_facts SET access_count = access_count + 1, last_verified = ?1 WHERE id = ?2",
                rusqlite::params![Utc::now().to_rfc3339(), existing_id],
            )?;
            return Ok(());
        }

        let cat = serde_json::to_string(&fact.category).unwrap_or_default();
        self.db.execute(
            "INSERT INTO knowledge_facts (id, project_id, category, content, confidence, source, created_at, last_verified, access_count, superseded_by, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                fact.id,
                fact.project_id,
                cat.trim_matches('"'),
                fact.content,
                fact.confidence,
                fact.source,
                fact.created_at.to_rfc3339(),
                fact.last_verified.to_rfc3339(),
                fact.access_count,
                fact.superseded_by,
                hash,
            ],
        )?;
        Ok(())
    }

    pub fn query(
        &self,
        project_id: &str,
        category: Option<&FactCategory>,
        limit: usize,
    ) -> Vec<KnowledgeFact> {
        let mut results = Vec::new();
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match category {
            Some(cat) => {
                let cat_str = serde_json::to_string(cat).unwrap_or_default();
                let cat_clean = cat_str.trim_matches('"').to_string();
                (
                    "SELECT id, project_id, category, content, confidence, source, created_at, last_verified, access_count, superseded_by
                     FROM knowledge_facts WHERE project_id = ?1 AND category = ?2 AND superseded_by IS NULL
                     ORDER BY confidence DESC, access_count DESC LIMIT ?3"
                        .to_string(),
                    vec![
                        Box::new(project_id.to_string()),
                        Box::new(cat_clean),
                        Box::new(limit as i64),
                    ],
                )
            }
            None => (
                "SELECT id, project_id, category, content, confidence, source, created_at, last_verified, access_count, superseded_by
                 FROM knowledge_facts WHERE project_id = ?1 AND superseded_by IS NULL
                 ORDER BY confidence DESC, access_count DESC LIMIT ?2"
                    .to_string(),
                vec![
                    Box::new(project_id.to_string()),
                    Box::new(limit as i64),
                ],
            ),
        };

        if let Ok(mut stmt) = self.db.prepare(&sql) {
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                params_vec.iter().map(|p| p.as_ref()).collect();
            let _ = stmt
                .query_map(params_refs.as_slice(), |row| {
                    let cat_str: String = row.get(2)?;
                    let category = parse_fact_category(&cat_str);
                    Ok(KnowledgeFact {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        category,
                        content: row.get(3)?,
                        confidence: row.get(4)?,
                        source: row.get(5)?,
                        created_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(6)?,
                        )
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                        last_verified: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(7)?,
                        )
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                        access_count: row.get::<_, i64>(8)? as u32,
                        superseded_by: row.get(9)?,
                    })
                })
                .map(|rows| {
                    for fact in rows.flatten() {
                        results.push(fact);
                    }
                });
        }
        results
    }

    /// Mark an existing fact as superseded by a new one (contradiction resolution).
    pub fn supersede(&self, old_id: &str, new_id: &str) -> Result<(), rusqlite::Error> {
        self.db.execute(
            "UPDATE knowledge_facts SET superseded_by = ?1 WHERE id = ?2",
            rusqlite::params![new_id, old_id],
        )?;
        Ok(())
    }

    pub fn count(&self, project_id: &str) -> Result<usize, rusqlite::Error> {
        let mut stmt = self.db.prepare(
            "SELECT COUNT(*) FROM knowledge_facts WHERE project_id = ?1 AND superseded_by IS NULL",
        )?;
        let count: i64 = stmt.query_row(rusqlite::params![project_id], |row| row.get(0))?;
        Ok(count as usize)
    }

    fn find_by_hash(&self, hash: &str) -> Result<Option<String>, rusqlite::Error> {
        let mut stmt = self.db.prepare(
            "SELECT id FROM knowledge_facts WHERE content_hash = ?1 AND superseded_by IS NULL LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![hash])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }
}

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

fn parse_fact_category(s: &str) -> FactCategory {
    match s {
        "architecture" => FactCategory::Architecture,
        "convention" => FactCategory::Convention,
        "decision" => FactCategory::Decision,
        "pattern" => FactCategory::Pattern,
        "preference" => FactCategory::Preference,
        "constraint" => FactCategory::Constraint,
        _ => FactCategory::Learned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_knowledge() -> (tempfile::TempDir, PersistentKnowledge) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("knowledge.db");
        let pk = PersistentKnowledge::new(&db_path).unwrap();
        (dir, pk)
    }

    fn make_fact(id: &str, project_id: &str, category: FactCategory, content: &str) -> KnowledgeFact {
        KnowledgeFact {
            id: id.to_string(),
            project_id: project_id.to_string(),
            category,
            content: content.to_string(),
            confidence: 0.9,
            source: "test".to_string(),
            created_at: Utc::now(),
            last_verified: Utc::now(),
            access_count: 0,
            superseded_by: None,
        }
    }

    #[test]
    fn store_and_query_fact() {
        let (_dir, pk) = temp_knowledge();
        let fact = make_fact("f1", "proj1", FactCategory::Architecture, "Use microservices");
        pk.store(&fact).unwrap();

        let results = pk.query("proj1", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Use microservices");
        assert_eq!(results[0].category, FactCategory::Architecture);
    }

    #[test]
    fn query_filters_by_project() {
        let (_dir, pk) = temp_knowledge();
        pk.store(&make_fact("f1", "proj1", FactCategory::Decision, "A")).unwrap();
        pk.store(&make_fact("f2", "proj2", FactCategory::Decision, "B")).unwrap();

        let results = pk.query("proj1", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "f1");
    }

    #[test]
    fn query_filters_by_category() {
        let (_dir, pk) = temp_knowledge();
        pk.store(&make_fact("f1", "p1", FactCategory::Architecture, "Arch fact")).unwrap();
        pk.store(&make_fact("f2", "p1", FactCategory::Convention, "Conv fact")).unwrap();

        let results = pk.query("p1", Some(&FactCategory::Architecture), 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Arch fact");
    }

    #[test]
    fn deduplication_bumps_access_count() {
        let (_dir, pk) = temp_knowledge();
        let fact = make_fact("f1", "p1", FactCategory::Learned, "Same content");
        pk.store(&fact).unwrap();

        let duplicate = make_fact("f2", "p1", FactCategory::Learned, "Same content");
        pk.store(&duplicate).unwrap();

        let results = pk.query("p1", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "f1");
        assert_eq!(results[0].access_count, 1);
    }

    #[test]
    fn supersede_hides_old_fact() {
        let (_dir, pk) = temp_knowledge();
        pk.store(&make_fact("old", "p1", FactCategory::Decision, "Old decision")).unwrap();
        pk.store(&make_fact("new", "p1", FactCategory::Decision, "New decision")).unwrap();
        pk.supersede("old", "new").unwrap();

        let results = pk.query("p1", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "new");
    }

    #[test]
    fn count_reflects_active_facts() {
        let (_dir, pk) = temp_knowledge();
        pk.store(&make_fact("f1", "p1", FactCategory::Learned, "A")).unwrap();
        pk.store(&make_fact("f2", "p1", FactCategory::Learned, "B")).unwrap();
        pk.store(&make_fact("f3", "p1", FactCategory::Learned, "C")).unwrap();
        pk.supersede("f1", "f3").unwrap();

        assert_eq!(pk.count("p1").unwrap(), 2);
    }

    #[test]
    fn category_label_coverage() {
        let f = make_fact("x", "p", FactCategory::Architecture, "");
        assert_eq!(f.category_label(), "arch");
        let f = make_fact("x", "p", FactCategory::Convention, "");
        assert_eq!(f.category_label(), "conv");
        let f = make_fact("x", "p", FactCategory::Preference, "");
        assert_eq!(f.category_label(), "pref");
        let f = make_fact("x", "p", FactCategory::Constraint, "");
        assert_eq!(f.category_label(), "constraint");
    }

    #[test]
    fn query_respects_limit() {
        let (_dir, pk) = temp_knowledge();
        for i in 0..10 {
            pk.store(&make_fact(
                &format!("f{i}"),
                "p1",
                FactCategory::Learned,
                &format!("Fact number {i}"),
            ))
            .unwrap();
        }
        let results = pk.query("p1", None, 3);
        assert_eq!(results.len(), 3);
    }
}
