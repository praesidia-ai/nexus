//! `workflow_dag` — DAG-based multi-agent workflow engine.
//!
//! Originally the `nexus-workflow` crate, now embedded in `nexus-core`
//! so that `NexusState` can use it directly without an external dependency.

pub mod dag;
pub mod pipelines;
pub mod runner;

pub use dag::{StepId, WorkflowDag, WorkflowStep};
pub use pipelines::{
    app_build_dag, code_review_dag, deploy_dag, feature_dag, pipeline_by_name, security_audit_dag,
    PIPELINE_NAMES,
};
pub use runner::{StepOutput, WorkflowContext, WorkflowResult, WorkflowRunner};
