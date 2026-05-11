use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodeGenError {
    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Agent '{agent_id}' failed at iteration {iteration}: {reason}")]
    AgentFailed {
        agent_id: String,
        iteration: u32,
        reason: String,
    },

    #[error("Dependency cycle detected in agent DAG: {0}")]
    DependencyCycle(String),

    #[error("Unknown agent role: {0}")]
    UnknownRole(String),

    #[error("Generation timeout after {0}ms")]
    Timeout(u64),

    #[error("Deployment failed for target {target}: {reason}")]
    DeployFailed { target: String, reason: String },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("File write error: {path}: {source}")]
    FileWrite {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialization(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, CodeGenError>;
