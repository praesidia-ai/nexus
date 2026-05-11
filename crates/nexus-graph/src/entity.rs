use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    File,
    Function,
    Class,
    Module,
    Api,
    Database,
    Service,
    Person,
    Decision,
    Requirement,
    Bug,
    Feature,
    Tool,
    Agent,
    Custom(String),
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::File => write!(f, "file"),
            EntityType::Function => write!(f, "function"),
            EntityType::Class => write!(f, "class"),
            EntityType::Module => write!(f, "module"),
            EntityType::Api => write!(f, "api"),
            EntityType::Database => write!(f, "database"),
            EntityType::Service => write!(f, "service"),
            EntityType::Person => write!(f, "person"),
            EntityType::Decision => write!(f, "decision"),
            EntityType::Requirement => write!(f, "requirement"),
            EntityType::Bug => write!(f, "bug"),
            EntityType::Feature => write!(f, "feature"),
            EntityType::Tool => write!(f, "tool"),
            EntityType::Agent => write!(f, "agent"),
            EntityType::Custom(s) => write!(f, "custom:{s}"),
        }
    }
}
