use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation: Relation,
    pub properties: serde_json::Value,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub confidence: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    DependsOn,
    Imports,
    Calls,
    Implements,
    Contains,
    OwnedBy,
    CreatedBy,
    ModifiedBy,
    RelatesTo,
    Supersedes,
    ConflictsWith,
    BlockedBy,
    Triggers,
    Produces,
    Consumes,
    Custom(String),
}

impl Relation {
    pub fn is_conflict_pair(&self, other: &Relation) -> bool {
        matches!(
            (self, other),
            (Relation::DependsOn, Relation::BlockedBy)
                | (Relation::BlockedBy, Relation::DependsOn)
                | (Relation::Produces, Relation::Consumes)
                | (Relation::Consumes, Relation::Produces)
                | (Relation::Supersedes, Relation::DependsOn)
                | (Relation::DependsOn, Relation::Supersedes)
        )
    }
}

impl std::fmt::Display for Relation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Relation::DependsOn => write!(f, "depends_on"),
            Relation::Imports => write!(f, "imports"),
            Relation::Calls => write!(f, "calls"),
            Relation::Implements => write!(f, "implements"),
            Relation::Contains => write!(f, "contains"),
            Relation::OwnedBy => write!(f, "owned_by"),
            Relation::CreatedBy => write!(f, "created_by"),
            Relation::ModifiedBy => write!(f, "modified_by"),
            Relation::RelatesTo => write!(f, "relates_to"),
            Relation::Supersedes => write!(f, "supersedes"),
            Relation::ConflictsWith => write!(f, "conflicts_with"),
            Relation::BlockedBy => write!(f, "blocked_by"),
            Relation::Triggers => write!(f, "triggers"),
            Relation::Produces => write!(f, "produces"),
            Relation::Consumes => write!(f, "consumes"),
            Relation::Custom(s) => write!(f, "custom:{s}"),
        }
    }
}
