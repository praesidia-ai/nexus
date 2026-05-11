use chrono::Utc;
use uuid::Uuid;

use crate::edge::{Edge, Relation};
use crate::entity::{Entity, EntityType};

struct Pattern {
    regex: &'static str,
    entity_type: EntityType,
}

const PATTERNS: &[Pattern] = &[
    Pattern { regex: r#"(?:^|[\s`'"])([a-zA-Z_][\w/\-]*\.(?:rs|ts|tsx|js|jsx|py|go|java|rb|c|cpp|h|hpp|toml|yaml|yml|json|sql|sh|css|scss|html|md))\b"#, entity_type: EntityType::File },
    Pattern { regex: r"\bfn\s+([a-zA-Z_]\w*)\s*[<(]", entity_type: EntityType::Function },
    Pattern { regex: r"\basync\s+fn\s+([a-zA-Z_]\w*)\s*[<(]", entity_type: EntityType::Function },
    Pattern { regex: r"\bfunction\s+([a-zA-Z_]\w*)\s*[<(]", entity_type: EntityType::Function },
    Pattern { regex: r"\bdef\s+([a-zA-Z_]\w*)\s*\(", entity_type: EntityType::Function },
    Pattern { regex: r"\bclass\s+([A-Z]\w*)", entity_type: EntityType::Class },
    Pattern { regex: r"\bstruct\s+([A-Z]\w*)", entity_type: EntityType::Class },
    Pattern { regex: r"\benum\s+([A-Z]\w*)", entity_type: EntityType::Class },
    Pattern { regex: r"\btrait\s+([A-Z]\w*)", entity_type: EntityType::Class },
    Pattern { regex: r"\binterface\s+([A-Z]\w*)", entity_type: EntityType::Class },
    Pattern { regex: r"\bmod\s+([a-zA-Z_]\w*)", entity_type: EntityType::Module },
    Pattern { regex: r"(?:GET|POST|PUT|DELETE|PATCH)\s+(/[a-zA-Z0-9_/\-:{}]+)", entity_type: EntityType::Api },
    Pattern { regex: r#"["'](/api/[a-zA-Z0-9_/\-:{}]+)["']"#, entity_type: EntityType::Api },
    Pattern { regex: r"\b([a-zA-Z_]\w*(?:Service|Manager|Handler|Gateway|Client|Provider))\b", entity_type: EntityType::Service },
];

pub fn extract_entities(text: &str, project_id: &str) -> Vec<Entity> {
    let mut seen = std::collections::HashSet::new();
    let mut entities = Vec::new();
    let now = Utc::now();

    for pattern in PATTERNS {
        let re = match regex_lite::Regex::new(pattern.regex) {
            Ok(r) => r,
            Err(_) => continue,
        };

        for cap in re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let name = m.as_str().to_string();
                let key = (name.clone(), pattern.entity_type.clone());
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);

                entities.push(Entity {
                    id: Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    name,
                    entity_type: pattern.entity_type.clone(),
                    properties: serde_json::json!({}),
                    created_at: now,
                    updated_at: now,
                    source: "extraction".to_string(),
                    confidence: 0.7,
                });
            }
        }
    }

    entities
}

struct RelationPattern {
    regex: &'static str,
    relation: Relation,
}

const RELATION_PATTERNS: &[RelationPattern] = &[
    RelationPattern { regex: r"(\w+)\s+(?:imports?|uses?)\s+(\w+)", relation: Relation::Imports },
    RelationPattern { regex: r"(\w+)\s+(?:calls?|invokes?)\s+(\w+)", relation: Relation::Calls },
    RelationPattern { regex: r"(\w+)\s+(?:depends?\s+on|requires?)\s+(\w+)", relation: Relation::DependsOn },
    RelationPattern { regex: r"(\w+)\s+(?:implements?|extends?)\s+(\w+)", relation: Relation::Implements },
    RelationPattern { regex: r"(\w+)\s+(?:contains?|includes?)\s+(\w+)", relation: Relation::Contains },
    RelationPattern { regex: r"(\w+)\s+(?:creates?|produces?)\s+(\w+)", relation: Relation::Produces },
    RelationPattern { regex: r"(\w+)\s+(?:consumes?|reads?)\s+(\w+)", relation: Relation::Consumes },
    RelationPattern { regex: r"(\w+)\s+(?:supersedes?|replaces?)\s+(\w+)", relation: Relation::Supersedes },
    RelationPattern { regex: r"(\w+)\s+(?:conflicts?\s+with|contradicts?)\s+(\w+)", relation: Relation::ConflictsWith },
    RelationPattern { regex: r"(\w+)\s+(?:blocked?\s+by)\s+(\w+)", relation: Relation::BlockedBy },
    RelationPattern { regex: r"(\w+)\s+(?:triggers?|causes?)\s+(\w+)", relation: Relation::Triggers },
    RelationPattern { regex: r"(\w+)\s+(?:owned?\s+by|belongs?\s+to)\s+(\w+)", relation: Relation::OwnedBy },
    RelationPattern { regex: r"(\w+)\s+(?:modified?\s+by|changed?\s+by)\s+(\w+)", relation: Relation::ModifiedBy },
    RelationPattern { regex: r"(\w+)\s+(?:relates?\s+to|related\s+to)\s+(\w+)", relation: Relation::RelatesTo },
];

pub fn extract_relations(text: &str, entities: &[Entity]) -> Vec<Edge> {
    let entity_names: std::collections::HashSet<&str> =
        entities.iter().map(|e| e.name.as_str()).collect();
    let entity_by_name: std::collections::HashMap<&str, &Entity> =
        entities.iter().map(|e| (e.name.as_str(), e)).collect();

    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let now = Utc::now();

    for rp in RELATION_PATTERNS {
        let re = match regex_lite::Regex::new(rp.regex) {
            Ok(r) => r,
            Err(_) => continue,
        };

        for cap in re.captures_iter(text) {
            let source_name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let target_name = cap.get(2).map(|m| m.as_str()).unwrap_or("");

            if !entity_names.contains(source_name) || !entity_names.contains(target_name) {
                continue;
            }
            if source_name == target_name {
                continue;
            }

            let key = (source_name.to_string(), target_name.to_string(), rp.relation.clone());
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            let source_entity = entity_by_name[source_name];
            let target_entity = entity_by_name[target_name];

            edges.push(Edge {
                id: Uuid::new_v4().to_string(),
                source_id: source_entity.id.clone(),
                target_id: target_entity.id.clone(),
                relation: rp.relation.clone(),
                properties: serde_json::json!({}),
                valid_from: now,
                valid_until: None,
                confidence: 0.6,
                source: "extraction".to_string(),
            });
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_file_entities() {
        let text = "The main logic is in `main.rs` and the library code in lib.rs";
        let entities = extract_entities(text, "proj1");
        let file_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::File)
            .map(|e| e.name.as_str())
            .collect();
        assert!(file_names.contains(&"main.rs"));
        assert!(file_names.contains(&"lib.rs"));
    }

    #[test]
    fn extract_function_entities() {
        let text = "fn parse_config(path: &str) -> Config {\n  async fn fetch_data() {}\n}";
        let entities = extract_entities(text, "proj1");
        let fn_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Function)
            .map(|e| e.name.as_str())
            .collect();
        assert!(fn_names.contains(&"parse_config"));
        assert!(fn_names.contains(&"fetch_data"));
    }

    #[test]
    fn extract_class_entities() {
        let text = "struct GraphStore {}\nenum EntityType {}\ntrait Queryable {}";
        let entities = extract_entities(text, "proj1");
        let class_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Class)
            .map(|e| e.name.as_str())
            .collect();
        assert!(class_names.contains(&"GraphStore"));
        assert!(class_names.contains(&"EntityType"));
        assert!(class_names.contains(&"Queryable"));
    }

    #[test]
    fn extract_api_entities() {
        let text = r#"GET /api/v1/users endpoint and POST /api/agents/tasks"#;
        let entities = extract_entities(text, "proj1");
        let api_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Api)
            .map(|e| e.name.as_str())
            .collect();
        assert!(api_names.contains(&"/api/v1/users"));
        assert!(api_names.contains(&"/api/agents/tasks"));
    }

    #[test]
    fn extract_service_entities() {
        let text = "The AuthService handles login and the TaskManager distributes work";
        let entities = extract_entities(text, "proj1");
        let svc_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Service)
            .map(|e| e.name.as_str())
            .collect();
        assert!(svc_names.contains(&"AuthService"));
        assert!(svc_names.contains(&"TaskManager"));
    }

    #[test]
    fn extract_module_entities() {
        let text = "mod store;\nmod extract;";
        let entities = extract_entities(text, "proj1");
        let mod_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Module)
            .map(|e| e.name.as_str())
            .collect();
        assert!(mod_names.contains(&"store"));
        assert!(mod_names.contains(&"extract"));
    }

    #[test]
    fn deduplicates_entities() {
        let text = "fn parse() {} fn parse() {}";
        let entities = extract_entities(text, "proj1");
        let parse_count = entities
            .iter()
            .filter(|e| e.name == "parse" && e.entity_type == EntityType::Function)
            .count();
        assert_eq!(parse_count, 1);
    }

    #[test]
    fn extract_relations_from_text() {
        let text = "AuthService imports UserStore. AuthService calls Validator.";
        let entities = vec![
            Entity {
                id: "e1".to_string(),
                project_id: "proj1".to_string(),
                name: "AuthService".to_string(),
                entity_type: EntityType::Service,
                properties: serde_json::json!({}),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                source: "test".to_string(),
                confidence: 1.0,
            },
            Entity {
                id: "e2".to_string(),
                project_id: "proj1".to_string(),
                name: "UserStore".to_string(),
                entity_type: EntityType::Service,
                properties: serde_json::json!({}),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                source: "test".to_string(),
                confidence: 1.0,
            },
            Entity {
                id: "e3".to_string(),
                project_id: "proj1".to_string(),
                name: "Validator".to_string(),
                entity_type: EntityType::Service,
                properties: serde_json::json!({}),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                source: "test".to_string(),
                confidence: 1.0,
            },
        ];

        let edges = extract_relations(text, &entities);
        assert!(edges.iter().any(|e| e.relation == Relation::Imports));
        assert!(edges.iter().any(|e| e.relation == Relation::Calls));
    }

    #[test]
    fn extract_relations_skips_unknown_entities() {
        let text = "FooService imports BarService";
        let entities = vec![Entity {
            id: "e1".to_string(),
            project_id: "proj1".to_string(),
            name: "FooService".to_string(),
            entity_type: EntityType::Service,
            properties: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source: "test".to_string(),
            confidence: 1.0,
        }];

        let edges = extract_relations(text, &entities);
        assert!(edges.is_empty());
    }
}
