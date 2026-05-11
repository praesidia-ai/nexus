//! nexus-kernel — Agent OS kernel providing Layers 1-3.
//!
//! This crate implements the core operating system primitives for the Nexus Agent OS:
//!
//! - **Layer 1: Process Model** — Agent processes with lifecycle management, resource
//!   allocation, priority scheduling, and parent/child relationships.
//!
//! - **Layer 2: Event Bus** — A publish/subscribe system with channel-based routing,
//!   glob pattern matching, event history, and subscription handlers for waking agents,
//!   delivering messages, or forwarding to webhooks.
//!
//! - **Layer 3: Reactive Agents** — Declarative agent definitions that respond to
//!   triggers (schedules, events, file changes, webhooks) with automatic lifecycle
//!   management and restart policies.

pub mod process;
pub mod scheduler;
pub mod events;
pub mod reactive;
pub mod reactive_manager;
pub mod builtins;

pub use process::{
    AgentProcess, ProcessContext, ProcessId, ProcessInfo, ProcessState, Priority,
    ResourceAllocation, ToolPermissions, NetworkPermissions, FilePermissions, WakeCondition,
};
pub use scheduler::{
    AgentScheduler, HealthStatus, ProcessGroup, ProcessHealth, ResourceSummary, ResourceUsage,
    SchedulerConfig,
};
pub use events::{
    EventBus, EventBusConfig, EventSource, ProcessEvent, Subscription,
    SubscriptionHandler, SystemEvent,
};
pub use reactive::{
    FileEvent, ReactiveAgentDefinition, ReactiveLifecycle, RestartPolicy, Trigger,
};
pub use reactive_manager::ReactiveAgentManager;

pub mod audit;
pub mod collaboration;
pub mod federation;
pub mod agentfs;
pub mod identity;
pub mod packages;
pub mod ui_runtime;

pub use collaboration::{
    CollaborationManager, CollaborationMessage, CollaborationSession,
    Participant, ParticipantRole,
};
pub use federation::{
    FederationLink, FederationManager, FederationTrust, SharedCapability,
};
pub use agentfs::{AgentFS, AgentFsError, FileInfo, FileOperation};
pub use audit::{AuditEntry, AuditKeypair, AuditLog, CapabilityToken, SignedManifest};
pub use identity::{AgentIdentity, Capability, IdentityError, PathScope, TrustLevel};
pub use packages::{NxaManifest, NxaPackage, PackageError, PackageManager};
pub use ui_runtime::{
    AgentWidget, ChartType, FormField, MessageFormat, ProgressStep, TableColumn,
    WidgetAction, WidgetError, WidgetManager, WidgetPosition, WidgetUpdate, WidgetUpdateType,
};
