//! Agent roster — the 10 named specialist agents and their roles.
//!
//! Each `AgentName` maps to a well-defined `AgentRole` that carries the
//! identity config (display name, domain, system-prompt seed, and default
//! tool allowlist) used when spawning the ZeroClaw agent instance.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AgentName
// ---------------------------------------------------------------------------

/// The 10 named ZeroClaw agents in a Nexus deployment.
///
/// These names are used everywhere: workflow DAG steps, roster lookups,
/// Praesidia registration, and CLI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentName {
    Nova,   // Coder
    Atlas,  // Cloud / infra
    Kai,    // Researcher
    Luna,   // Writer / docs
    Orion,  // Security auditor
    Sage,   // Data analyst
    Ivy,    // Marketer
    Rex,    // DevOps
    Leo,    // Product manager
    Mia,    // Customer support
}

impl AgentName {
    /// Human-readable display name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Nova  => "Nova",
            Self::Atlas => "Atlas",
            Self::Kai   => "Kai",
            Self::Luna  => "Luna",
            Self::Orion => "Orion",
            Self::Sage  => "Sage",
            Self::Ivy   => "Ivy",
            Self::Rex   => "Rex",
            Self::Leo   => "Leo",
            Self::Mia   => "Mia",
        }
    }

    /// All agents in definition order.
    pub fn all() -> &'static [AgentName] {
        use AgentName::*;
        &[Nova, Atlas, Kai, Luna, Orion, Sage, Ivy, Rex, Leo, Mia]
    }

    /// The specialist role descriptor for this agent.
    pub fn role(self) -> AgentRole {
        match self {
            Self::Nova  => AgentRole {
                domain: "Software Engineering",
                tagline: "Full-stack coder: architecture, implementation, tests, refactoring",
                system_prompt_seed: "You are Nova, an expert software engineer. \
                    You write clean, production-ready code in any language. \
                    You scaffold projects, implement features, write tests, and refactor. \
                    Always prefer simplicity, correctness, and idiomatic patterns.",
                default_tools: &["shell", "file", "git", "web_fetch"],
            },
            Self::Atlas => AgentRole {
                domain: "Cloud & Infrastructure",
                tagline: "Cloud architect: AWS/GCP/Azure, IaC, containers, scaling",
                system_prompt_seed: "You are Atlas, a cloud and infrastructure expert. \
                    You design and implement cloud architectures, write Terraform/Pulumi IaC, \
                    configure Kubernetes, and optimize cost and reliability.",
                default_tools: &["shell", "file", "web_fetch"],
            },
            Self::Kai   => AgentRole {
                domain: "Research",
                tagline: "Researcher: deep web research, competitive analysis, synthesis",
                system_prompt_seed: "You are Kai, a meticulous researcher. \
                    You gather information from multiple sources, synthesize findings, \
                    and produce clear, well-cited summaries and recommendations.",
                default_tools: &["web_fetch", "web_search", "file"],
            },
            Self::Luna  => AgentRole {
                domain: "Technical Writing",
                tagline: "Writer: READMEs, API docs, user guides, blog posts",
                system_prompt_seed: "You are Luna, a skilled technical writer. \
                    You produce clear, accurate, and engaging documentation, \
                    READMEs, API references, tutorials, and blog posts.",
                default_tools: &["file", "web_fetch"],
            },
            Self::Orion => AgentRole {
                domain: "Security",
                tagline: "Security auditor: OWASP, threat modelling, pen-test, hardening",
                system_prompt_seed: "You are Orion, a security specialist. \
                    You audit code and infrastructure for vulnerabilities, model threats, \
                    recommend mitigations, and verify fixes against security best-practices.",
                default_tools: &["shell", "file", "web_fetch"],
            },
            Self::Sage  => AgentRole {
                domain: "Data & Analytics",
                tagline: "Data analyst: SQL, ETL, visualisation, ML pipelines",
                system_prompt_seed: "You are Sage, a data and analytics expert. \
                    You design data models, write SQL, build ETL pipelines, \
                    create visualisations, and prototype ML workflows.",
                default_tools: &["shell", "file", "web_fetch"],
            },
            Self::Ivy   => AgentRole {
                domain: "Marketing",
                tagline: "Marketer: copy, SEO, campaigns, positioning, growth",
                system_prompt_seed: "You are Ivy, a marketing strategist and copywriter. \
                    You craft compelling messaging, SEO-optimised content, \
                    campaign plans, and growth strategies.",
                default_tools: &["web_fetch", "web_search", "file"],
            },
            Self::Rex   => AgentRole {
                domain: "DevOps",
                tagline: "DevOps engineer: CI/CD, Docker, monitoring, deployment",
                system_prompt_seed: "You are Rex, a DevOps engineer. \
                    You configure CI/CD pipelines, write Dockerfiles and Compose files, \
                    set up monitoring and alerting, and automate deployments.",
                default_tools: &["shell", "file", "git", "web_fetch"],
            },
            Self::Leo   => AgentRole {
                domain: "Product Management",
                tagline: "PM: requirements, user stories, roadmaps, prioritisation",
                system_prompt_seed: "You are Leo, a product manager. \
                    You gather and refine requirements, write user stories, \
                    build roadmaps, and keep the team focused on user value.",
                default_tools: &["file", "web_fetch"],
            },
            Self::Mia   => AgentRole {
                domain: "Customer Support",
                tagline: "Support specialist: FAQs, escalation paths, empathy-first comms",
                system_prompt_seed: "You are Mia, a customer support specialist. \
                    You handle user queries with empathy and precision, \
                    write help-centre articles, and design support workflows.",
                default_tools: &["file", "web_fetch"],
            },
        }
    }
}

impl std::fmt::Display for AgentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

impl std::str::FromStr for AgentName {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "nova"  => Ok(Self::Nova),
            "atlas" => Ok(Self::Atlas),
            "kai"   => Ok(Self::Kai),
            "luna"  => Ok(Self::Luna),
            "orion" => Ok(Self::Orion),
            "sage"  => Ok(Self::Sage),
            "ivy"   => Ok(Self::Ivy),
            "rex"   => Ok(Self::Rex),
            "leo"   => Ok(Self::Leo),
            "mia"   => Ok(Self::Mia),
            other   => anyhow::bail!("unknown agent name: '{}'", other),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentRole
// ---------------------------------------------------------------------------

/// Static role descriptor for a named agent.
#[derive(Debug, Clone, Copy)]
pub struct AgentRole {
    /// High-level domain label.
    pub domain: &'static str,
    /// One-line description shown in `nexus status` and the Praesidia portal.
    pub tagline: &'static str,
    /// Seed text injected as the ZeroClaw system prompt for this agent.
    pub system_prompt_seed: &'static str,
    /// Tools the agent is allowed to invoke by default.
    pub default_tools: &'static [&'static str],
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_agents_parse_from_display() {
        for name in AgentName::all() {
            let s = name.display_name().to_lowercase();
            let parsed: AgentName = s.parse().expect("parse");
            assert_eq!(*name, parsed);
        }
    }

    #[test]
    fn roles_have_non_empty_fields() {
        for name in AgentName::all() {
            let role = name.role();
            assert!(!role.domain.is_empty());
            assert!(!role.tagline.is_empty());
            assert!(!role.system_prompt_seed.is_empty());
        }
    }
}
