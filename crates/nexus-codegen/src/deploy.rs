use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub target: DeployTarget,
    pub env_vars: HashMap<String, String>,
    pub build_command: String,
    pub output_dir: String,
    pub health_check: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeployTarget {
    Vercel {
        project_name: String,
        team: Option<String>,
    },
    Railway {
        project_id: Option<String>,
    },
    FlyIo {
        app_name: String,
        region: String,
    },
    Docker {
        registry: String,
        tag: String,
    },
    Local {
        port: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionConfig {
    pub database: Option<DatabaseProvider>,
    pub auth: Option<AuthProvider>,
    pub payments: Option<PaymentProvider>,
    pub storage: Option<StorageProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseProvider {
    Supabase,
    PlanetScale,
    Neon,
    Railway,
    LocalSqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    NextAuth,
    Clerk,
    Supabase,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentProvider {
    Stripe,
    StripeConnect,
    LemonSqueezy,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StorageProvider {
    Cloudinary,
    S3,
    Supabase,
    Local,
}

impl DeployConfig {
    pub fn vercel(project_name: &str) -> Self {
        Self {
            target: DeployTarget::Vercel {
                project_name: project_name.to_string(),
                team: None,
            },
            env_vars: HashMap::new(),
            build_command: "npm run build".to_string(),
            output_dir: ".next".to_string(),
            health_check: Some("/api/health".to_string()),
        }
    }

    pub fn railway() -> Self {
        Self {
            target: DeployTarget::Railway { project_id: None },
            env_vars: HashMap::new(),
            build_command: "npm run build".to_string(),
            output_dir: ".next".to_string(),
            health_check: Some("/api/health".to_string()),
        }
    }

    pub fn docker(registry: &str, tag: &str) -> Self {
        Self {
            target: DeployTarget::Docker {
                registry: registry.to_string(),
                tag: tag.to_string(),
            },
            env_vars: HashMap::new(),
            build_command: "docker build -t app .".to_string(),
            output_dir: ".".to_string(),
            health_check: Some("/api/health".to_string()),
        }
    }

    pub fn local(port: u16) -> Self {
        Self {
            target: DeployTarget::Local { port },
            env_vars: HashMap::new(),
            build_command: "npm run build".to_string(),
            output_dir: ".next".to_string(),
            health_check: Some(format!("http://localhost:{}/api/health", port)),
        }
    }
}

impl ProvisionConfig {
    pub fn full_saas() -> Self {
        Self {
            database: Some(DatabaseProvider::Supabase),
            auth: Some(AuthProvider::NextAuth),
            payments: Some(PaymentProvider::Stripe),
            storage: Some(StorageProvider::Supabase),
        }
    }

    pub fn minimal() -> Self {
        Self {
            database: Some(DatabaseProvider::LocalSqlite),
            auth: Some(AuthProvider::Custom),
            payments: None,
            storage: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vercel_deploy_config() {
        let cfg = DeployConfig::vercel("my-app");
        match &cfg.target {
            DeployTarget::Vercel { project_name, team } => {
                assert_eq!(project_name, "my-app");
                assert!(team.is_none());
            }
            _ => panic!("Expected Vercel target"),
        }
        assert_eq!(cfg.build_command, "npm run build");
        assert_eq!(cfg.health_check, Some("/api/health".to_string()));
    }

    #[test]
    fn railway_deploy_config() {
        let cfg = DeployConfig::railway();
        assert!(matches!(cfg.target, DeployTarget::Railway { project_id: None }));
    }

    #[test]
    fn docker_deploy_config() {
        let cfg = DeployConfig::docker("ghcr.io/org", "latest");
        match &cfg.target {
            DeployTarget::Docker { registry, tag } => {
                assert_eq!(registry, "ghcr.io/org");
                assert_eq!(tag, "latest");
            }
            _ => panic!("Expected Docker target"),
        }
    }

    #[test]
    fn local_deploy_config() {
        let cfg = DeployConfig::local(3000);
        match &cfg.target {
            DeployTarget::Local { port } => assert_eq!(*port, 3000),
            _ => panic!("Expected Local target"),
        }
        assert!(cfg.health_check.unwrap().contains("3000"));
    }

    #[test]
    fn full_saas_provision() {
        let cfg = ProvisionConfig::full_saas();
        assert_eq!(cfg.database, Some(DatabaseProvider::Supabase));
        assert_eq!(cfg.auth, Some(AuthProvider::NextAuth));
        assert_eq!(cfg.payments, Some(PaymentProvider::Stripe));
        assert_eq!(cfg.storage, Some(StorageProvider::Supabase));
    }

    #[test]
    fn minimal_provision() {
        let cfg = ProvisionConfig::minimal();
        assert_eq!(cfg.database, Some(DatabaseProvider::LocalSqlite));
        assert_eq!(cfg.auth, Some(AuthProvider::Custom));
        assert!(cfg.payments.is_none());
        assert!(cfg.storage.is_none());
    }

    #[test]
    fn deploy_config_serialization_roundtrip() {
        let cfg = DeployConfig::vercel("test-project");
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: DeployConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.target, cfg.target);
        assert_eq!(deserialized.build_command, cfg.build_command);
    }

    #[test]
    fn deploy_target_serializes_to_snake_case() {
        let target = DeployTarget::FlyIo {
            app_name: "test".to_string(),
            region: "iad".to_string(),
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("fly_io"));
    }
}
