//! `praesidia-nexus` — Praesidia policy engine and cloud client.
//!
//! ## What's in this crate
//!
//! | Module | Purpose |
//! |---|---|
//! | `client` | HTTP client for the Praesidia platform API (publish, register, health) |
//! | `policy` | `PraesidiaPolicy` YAML type |
//! | `policy_eval` | Stateless ingress / egress / tool evaluation |
//! | `policy_hash` | SHA-256 helpers |
//! | `policy_hook` | `PraesidiaPolicyHook` — implements the agent lifecycle hooks |
//! | `zeroclaw` | ZeroClaw runtime registry helpers (runtime.json discovery) |

pub mod client;
pub mod policy;
pub mod policy_eval;
pub mod policy_hash;
pub mod policy_hook;
pub mod zeroclaw;

pub use client::{PraesidiaClient, PublishProjectRequest, PublishResult, RegisterAgentRequest};
pub use policy::*;
pub use policy_eval::{eval_egress, eval_ingress, eval_tool, HookResult};
pub use policy_hash::sha256_hex;
pub use policy_hook::PraesidiaPolicyHook;
pub use zeroclaw::{
    clear_runtime, get_agent_url, list_registered_agents, load_runtime, load_runtime_at,
    ping_agent, runtime_file_at_base, save_runtime, HealthResponse, ZeroClawAgentEntry,
    ZeroClawRegistration,
};
