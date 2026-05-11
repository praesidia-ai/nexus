use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("capability denied: {0}")]
    CapabilityDenied(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrustLevel {
    ReadOnly,
    Standard,
    Elevated,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PathScope {
    All,
    ProjectOnly,
    ScopedTo(Vec<PathBuf>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Capability {
    ReadFiles { scope: PathScope },
    WriteFiles { scope: PathScope },
    DeleteFiles { scope: PathScope },
    RunCommands { allowed: Option<Vec<String>> },
    InstallPackages,
    BuildProject,
    HttpRequests { domains: Option<Vec<String>> },
    DatabaseRead,
    DatabaseWrite,
    SendEmail,
    GitHubAccess { repos: Vec<String> },
    SpawnAgents { max: u32 },
    KillAgents,
    AccessEventBus,
    PublishEvents { channels: Vec<String> },
    CallLlm { max_cost: Option<f32> },
    GenerateImages,
    ManageRuntime,
    AccessSecrets { keys: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
}

impl AgentIdentity {
    pub fn has_capability(&self, cap: &Capability) -> bool {
        if self.trust_level == TrustLevel::System {
            return true;
        }
        self.capabilities.iter().any(|c| capability_covers(c, cap))
    }

    pub fn check_capability(&self, cap: &Capability) -> Result<(), IdentityError> {
        if self.has_capability(cap) {
            Ok(())
        } else {
            Err(IdentityError::CapabilityDenied(format!(
                "Agent {} lacks {:?}",
                self.name, cap
            )))
        }
    }

    pub fn default_for_trust(trust: &TrustLevel) -> Vec<Capability> {
        match trust {
            TrustLevel::ReadOnly => vec![Capability::ReadFiles {
                scope: PathScope::ProjectOnly,
            }],
            TrustLevel::Standard => vec![
                Capability::ReadFiles {
                    scope: PathScope::ProjectOnly,
                },
                Capability::WriteFiles {
                    scope: PathScope::ProjectOnly,
                },
                Capability::BuildProject,
                Capability::CallLlm {
                    max_cost: Some(1.0),
                },
            ],
            TrustLevel::Elevated => vec![
                Capability::ReadFiles {
                    scope: PathScope::All,
                },
                Capability::WriteFiles {
                    scope: PathScope::All,
                },
                Capability::RunCommands { allowed: None },
                Capability::HttpRequests { domains: None },
                Capability::CallLlm { max_cost: None },
                Capability::SpawnAgents { max: 5 },
            ],
            TrustLevel::System => vec![], // System has all capabilities implicitly
        }
    }
}

fn capability_covers(held: &Capability, required: &Capability) -> bool {
    std::mem::discriminant(held) == std::mem::discriminant(required)
}
