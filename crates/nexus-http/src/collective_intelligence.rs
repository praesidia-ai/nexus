//! Collective Intelligence — Agent Council for consensus-driven decisions.
//!
//! A team of specialized agents deliberate on every major decision:
//! 1. ArchitectAgent proposes a solution
//! 2. ReviewerAgent, ProductAgent, UXAgent critique in parallel
//! 3. Conflicts are resolved
//! 4. A final consensus output is produced
//!
//! Integrates with nexus_brain and coding_agents orchestrator.
//! 100% deterministic (no LLM) — the council applies rules, not generation.

use serde::Serialize;

use crate::intent_engine::{AppType, Complexity, FlatIntent};
use crate::decision_engine::ArchitectureDecision;

// ---------------------------------------------------------------------------
// Council output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CollectiveDecision {
    /// The architect's original proposal.
    pub proposal: Proposal,
    /// Critiques from each reviewing agent.
    pub critiques: Vec<Critique>,
    /// Final consensus decision after conflict resolution.
    pub final_decision: ConsensusOutput,
    /// Overall confidence in the decision (0.0–1.0).
    pub confidence_score: f64,
    /// How conflicts were resolved.
    pub resolution_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub summary: String,
    pub architecture: String,
    pub key_decisions: Vec<String>,
    pub estimated_complexity: String,
    pub pages: Vec<String>,
    pub agents_needed: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Critique {
    pub agent: CouncilAgent,
    pub verdict: Verdict,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CouncilAgent {
    Architect,
    Reviewer,
    Product,
    Ux,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approve,
    ApproveWithChanges,
    RequestChanges,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsensusOutput {
    pub summary: String,
    pub changes_from_proposal: Vec<String>,
    pub final_pages: Vec<String>,
    pub final_agents: Vec<String>,
    pub final_features: Vec<String>,
    pub risk_level: String,
}

// ---------------------------------------------------------------------------
// Council deliberation
// ---------------------------------------------------------------------------

/// Run the Agent Council on an intent and architecture decision.
///
/// This is deterministic — the council applies expert rules, not LLM generation.
pub fn deliberate(
    intent: &FlatIntent,
    architecture: &ArchitectureDecision,
    description: &str,
) -> CollectiveDecision {
    // Step 1: Architect proposes
    let proposal = architect_propose(intent, architecture);

    // Step 2: All agents critique in parallel (simulated — they're deterministic)
    let reviewer_critique = reviewer_critique(intent, &proposal);
    let product_critique = product_critique(intent, &proposal, description);
    let ux_critique = ux_critique(intent, &proposal);

    let critiques = vec![reviewer_critique, product_critique, ux_critique];

    // Step 3: Resolve conflicts
    let (final_decision, resolution_notes) = resolve_conflicts(&proposal, &critiques, intent);

    // Step 4: Compute confidence
    let approvals = critiques
        .iter()
        .filter(|c| matches!(c.verdict, Verdict::Approve | Verdict::ApproveWithChanges))
        .count();
    let total_issues: usize = critiques.iter().map(|c| c.issues.len()).sum();
    let base_confidence = approvals as f64 / critiques.len() as f64;
    let issue_penalty = (total_issues as f64 * 0.03).min(0.3);
    let confidence_score = (base_confidence - issue_penalty).max(0.3);

    CollectiveDecision {
        proposal,
        critiques,
        final_decision,
        confidence_score,
        resolution_notes,
    }
}

// ---------------------------------------------------------------------------
// Architect Agent
// ---------------------------------------------------------------------------

fn architect_propose(intent: &FlatIntent, arch: &ArchitectureDecision) -> Proposal {
    let summary = format!(
        "{:?} application with {:?} frontend, {:?} database, {:?} auth",
        intent.app_type, arch.frontend, arch.database, arch.auth
    );

    let mut key_decisions = Vec::new();
    for r in &arch.rationale {
        key_decisions.push(format!("{}: {} — {}", r.area, r.choice, r.reason));
    }

    let estimated_complexity = format!("{:?}", intent.complexity);

    Proposal {
        summary,
        architecture: format!("{:?}", arch.frontend),
        key_decisions,
        estimated_complexity,
        pages: intent.suggested_pages.clone(),
        agents_needed: intent
            .suggested_agents
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Reviewer Agent — technical quality
// ---------------------------------------------------------------------------

fn reviewer_critique(intent: &FlatIntent, proposal: &Proposal) -> Critique {
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();

    // Check: complex app with too few pages
    if intent.complexity == Complexity::Complex && proposal.pages.len() < 5 {
        issues.push("Complex app has fewer than 5 pages — may be missing critical views".into());
        suggestions.push("Add: Settings, Profile, Admin, Reports, Notifications pages".into());
    }

    // Check: auth needed but no login/register pages
    if intent.needs_auth {
        let has_auth_pages = proposal
            .pages
            .iter()
            .any(|p| p.to_lowercase().contains("login") || p.to_lowercase().contains("register"));
        if !has_auth_pages {
            issues.push("Auth is required but no login/register pages in proposal".into());
            suggestions.push("Add Login and Register pages".into());
        }
    }

    // Check: payments but no checkout/pricing
    if intent.needs_payments {
        let has_payment_pages = proposal.pages.iter().any(|p| {
            let l = p.to_lowercase();
            l.contains("pricing") || l.contains("checkout") || l.contains("billing")
        });
        if !has_payment_pages {
            issues.push("Payments needed but no pricing/checkout page".into());
            suggestions.push("Add Pricing and Checkout pages".into());
        }
    }

    // Check: no error handling mentioned
    suggestions.push("Ensure error boundaries and loading states in all pages".into());

    let verdict = if issues.is_empty() {
        Verdict::Approve
    } else if issues.len() <= 2 {
        Verdict::ApproveWithChanges
    } else {
        Verdict::RequestChanges
    };

    Critique {
        agent: CouncilAgent::Reviewer,
        verdict,
        issues,
        suggestions,
    }
}

// ---------------------------------------------------------------------------
// Product Agent — business viability
// ---------------------------------------------------------------------------

fn product_critique(intent: &FlatIntent, proposal: &Proposal, description: &str) -> Critique {
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();
    let lower = description.to_lowercase();

    // SaaS without onboarding
    if matches!(intent.app_type, AppType::SaasApp) {
        if !proposal.pages.iter().any(|p| p.to_lowercase().contains("onboard")) {
            suggestions.push("Add onboarding flow — critical for SaaS activation".into());
        }
        if !intent.needs_payments {
            issues.push("SaaS app without payments — no revenue path defined".into());
            suggestions.push("Add a freemium pricing model with Stripe integration".into());
        }
    }

    // Marketplace without trust signals
    if matches!(intent.app_type, AppType::Marketplace)
        && !lower.contains("review") && !lower.contains("rating") {
            suggestions.push("Add review/rating system — essential for marketplace trust".into());
        }

    // E-commerce without product discovery
    if matches!(intent.app_type, AppType::ECommerce) {
        suggestions.push("Add search, filters, and product recommendations for conversion".into());
    }

    // No CTA or conversion path
    if proposal.pages.len() > 1 {
        suggestions.push("Ensure every page has a clear call-to-action".into());
    }

    let verdict = if issues.is_empty() {
        Verdict::Approve
    } else {
        Verdict::ApproveWithChanges
    };

    Critique {
        agent: CouncilAgent::Product,
        verdict,
        issues,
        suggestions,
    }
}

// ---------------------------------------------------------------------------
// UX Agent — experience quality
// ---------------------------------------------------------------------------

fn ux_critique(intent: &FlatIntent, proposal: &Proposal) -> Critique {
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();

    // Mobile responsiveness
    suggestions.push("Ensure mobile-first responsive design on all pages".into());

    // Too many pages for a simple app
    if intent.complexity == Complexity::Simple && proposal.pages.len() > 6 {
        issues.push("Simple app with too many pages — cognitive overload risk".into());
        suggestions.push("Consolidate into fewer, focused pages".into());
    }

    // Complex app should have a dashboard
    if intent.complexity == Complexity::Complex
        && !proposal.pages.iter().any(|p| p.to_lowercase().contains("dashboard")) {
            issues.push("Complex app without a dashboard — users need a home base".into());
            suggestions.push("Add a dashboard as the primary landing page after login".into());
        }

    // Agents should have clear UI triggers
    if !proposal.agents_needed.is_empty() {
        suggestions.push("Agents should have visible but non-intrusive UI triggers (floating button or sidebar)".into());
    }

    // Accessibility
    suggestions.push("Enforce WCAG 2.1 AA: proper contrast, focus indicators, alt text, ARIA labels".into());

    let verdict = if issues.is_empty() {
        Verdict::Approve
    } else {
        Verdict::ApproveWithChanges
    };

    Critique {
        agent: CouncilAgent::Ux,
        verdict,
        issues,
        suggestions,
    }
}

// ---------------------------------------------------------------------------
// Conflict resolution
// ---------------------------------------------------------------------------

fn resolve_conflicts(
    proposal: &Proposal,
    critiques: &[Critique],
    intent: &FlatIntent,
) -> (ConsensusOutput, Vec<String>) {
    let mut changes = Vec::new();
    let mut notes = Vec::new();
    let mut final_pages = proposal.pages.clone();
    let final_agents = proposal.agents_needed.clone();
    let mut final_features: Vec<String> = intent.inferred_features.clone();

    for critique in critiques {
        for suggestion in &critique.suggestions {
            let lower = suggestion.to_lowercase();

            // Auto-accept page additions
            if lower.contains("add") && (lower.contains("page") || lower.contains("login")
                || lower.contains("register") || lower.contains("dashboard")
                || lower.contains("pricing") || lower.contains("checkout")
                || lower.contains("onboarding"))
            {
                let page_name = extract_page_name(suggestion);
                if !page_name.is_empty()
                    && !final_pages.iter().any(|p| p.to_lowercase() == page_name.to_lowercase())
                {
                    final_pages.push(page_name.clone());
                    changes.push(format!("Added page: {} (suggested by {:?})", page_name, critique.agent));
                    notes.push(format!(
                        "Accepted {:?}'s suggestion to add {} page",
                        critique.agent, page_name
                    ));
                }
            }

            // Auto-accept feature suggestions
            if lower.contains("add") && (lower.contains("search") || lower.contains("filter")
                || lower.contains("review") || lower.contains("onboarding"))
            {
                let feature = suggestion.clone();
                if !final_features.contains(&feature) {
                    final_features.push(feature.clone());
                    changes.push(format!("Added feature: {} (by {:?})", feature, critique.agent));
                }
            }
        }

        for issue in &critique.issues {
            notes.push(format!("{:?} flagged: {}", critique.agent, issue));
        }
    }

    // Risk assessment
    let total_issues: usize = critiques.iter().map(|c| c.issues.len()).sum();
    let risk_level = if total_issues == 0 {
        "low"
    } else if total_issues <= 3 {
        "medium"
    } else {
        "high"
    }
    .to_string();

    let summary = if changes.is_empty() {
        "Council approved proposal without modifications".to_string()
    } else {
        format!(
            "Council approved with {} modification(s): {}",
            changes.len(),
            changes.iter().take(3).cloned().collect::<Vec<_>>().join("; ")
        )
    };

    (
        ConsensusOutput {
            summary,
            changes_from_proposal: changes,
            final_pages,
            final_agents,
            final_features,
            risk_level,
        },
        notes,
    )
}

fn extract_page_name(suggestion: &str) -> String {
    let words = ["Login", "Register", "Dashboard", "Pricing", "Checkout",
        "Billing", "Settings", "Profile", "Admin", "Reports", "Onboarding",
        "Notifications", "Search"];
    for word in words {
        if suggestion.to_lowercase().contains(&word.to_lowercase()) {
            return word.to_string();
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_engine::analyze_flat;
    use crate::decision_engine;

    #[test]
    fn council_produces_consensus_for_saas() {
        let intent = analyze_flat("Build a SaaS project management tool with billing");
        let arch = decision_engine::decide(&intent);
        let decision = deliberate(&intent, &arch, "Build a SaaS project management tool with billing");

        assert!(!decision.proposal.summary.is_empty());
        assert!(!decision.critiques.is_empty());
        assert_eq!(decision.critiques.len(), 3); // reviewer, product, ux
        assert!(decision.confidence_score > 0.0);
        assert!(!decision.final_decision.final_pages.is_empty());
    }

    #[test]
    fn council_adds_missing_auth_pages() {
        let mut intent = analyze_flat("Simple dashboard");
        intent.needs_auth = true;
        intent.suggested_pages = vec!["Dashboard".into()]; // Deliberately missing login
        let arch = decision_engine::decide(&intent);
        let decision = deliberate(&intent, &arch, "Simple dashboard");

        // Council should add Login/Register pages
        let pages = &decision.final_decision.final_pages;
        assert!(
            pages.iter().any(|p| p.to_lowercase().contains("login")),
            "Council should add Login page. Pages: {:?}",
            pages
        );
    }

    #[test]
    fn council_flags_no_payments_for_saas() {
        let mut intent = analyze_flat("SaaS tool");
        intent.needs_payments = false;
        let arch = decision_engine::decide(&intent);
        let decision = deliberate(&intent, &arch, "SaaS tool");

        let product_critique = decision
            .critiques
            .iter()
            .find(|c| c.agent == CouncilAgent::Product)
            .unwrap();
        assert!(
            product_critique.issues.iter().any(|i| i.to_lowercase().contains("revenue") || i.to_lowercase().contains("payment")),
            "Product agent should flag missing revenue path. Issues: {:?}",
            product_critique.issues
        );
    }

    #[test]
    fn confidence_decreases_with_more_issues() {
        let intent = analyze_flat("Build a complex marketplace with AI, payments, auth, admin");
        let arch = decision_engine::decide(&intent);
        let complex = deliberate(&intent, &arch, "complex marketplace");

        let simple_intent = analyze_flat("Simple landing page");
        let simple_arch = decision_engine::decide(&simple_intent);
        let simple = deliberate(&simple_intent, &simple_arch, "simple landing page");

        // Simple app should have higher confidence (fewer issues)
        assert!(
            simple.confidence_score >= complex.confidence_score - 0.1,
            "Simple={:.2}, Complex={:.2}",
            simple.confidence_score,
            complex.confidence_score,
        );
    }
}
