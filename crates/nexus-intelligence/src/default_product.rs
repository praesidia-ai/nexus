//! Default template-based implementation of [`ProductIntelligence`].
//!
//! Generates a product brief from intent analysis using domain-specific
//! templates and heuristic copy generation. No LLM calls.

use async_trait::async_trait;

use super::intent::{AppType, Intent};
use super::product::*;

/// Built-in template-based product intelligence.
///
/// Uses the app type, domain, and inferred features to assemble a
/// product brief with hero copy, feature list, personas, and optional
/// monetization strategy.
pub struct TemplateProductIntelligence;

impl TemplateProductIntelligence {
    pub fn new() -> Self {
        Self
    }

    fn generate_hero(&self, intent: &Intent) -> (String, String, String) {
        let domain = &intent.inferred.domain.value;
        let app_type = &intent.inferred.app_type.value;

        match app_type {
            AppType::LandingPage => (
                format!("The {domain} platform you've been waiting for"),
                "Built for teams who move fast and ship with confidence.".to_string(),
                "Get Started Free".to_string(),
            ),
            AppType::Dashboard => (
                format!("Your {domain} metrics, all in one place"),
                "Real-time insights that help you make better decisions.".to_string(),
                "View Dashboard".to_string(),
            ),
            AppType::Ecommerce => (
                format!("Shop the best in {domain}"),
                "Curated products, seamless checkout, fast delivery.".to_string(),
                "Start Shopping".to_string(),
            ),
            AppType::SaaS => (
                format!("Streamline your {domain} workflow"),
                "Automate the tedious parts. Focus on what matters.".to_string(),
                "Start Free Trial".to_string(),
            ),
            AppType::Blog => (
                format!("Insights on {domain}"),
                "Expert perspectives and practical guides.".to_string(),
                "Read Latest".to_string(),
            ),
            _ => (
                format!("Build better with {domain}"),
                "A modern platform designed for the way you work.".to_string(),
                "Get Started".to_string(),
            ),
        }
    }

    fn generate_personas(&self, intent: &Intent) -> Vec<Persona> {
        let domain = &intent.inferred.domain.value;

        vec![
            Persona {
                name: format!("Alex the {domain} enthusiast"),
                role: "Primary User".to_string(),
                goals: vec![
                    "Complete tasks quickly".to_string(),
                    "Track progress over time".to_string(),
                ],
                pain_points: vec![
                    "Current tools are fragmented".to_string(),
                    "Too much manual work".to_string(),
                ],
            },
            Persona {
                name: format!("Jordan the {domain} manager"),
                role: "Team Lead / Admin".to_string(),
                goals: vec![
                    "Oversee team activity".to_string(),
                    "Generate reports".to_string(),
                ],
                pain_points: vec![
                    "Lack of visibility into team progress".to_string(),
                    "Reporting is time-consuming".to_string(),
                ],
            },
        ]
    }

    fn generate_monetization(&self, intent: &Intent) -> Option<MonetizationStrategy> {
        match &intent.inferred.app_type.value {
            AppType::SaaS | AppType::Marketplace => Some(MonetizationStrategy {
                model: "freemium".to_string(),
                tiers: vec![
                    PricingTier {
                        name: "Free".to_string(),
                        price: "$0/mo".to_string(),
                        features: vec![
                            "Basic features".to_string(),
                            "Community support".to_string(),
                        ],
                    },
                    PricingTier {
                        name: "Pro".to_string(),
                        price: "$29/mo".to_string(),
                        features: vec![
                            "All Free features".to_string(),
                            "Advanced analytics".to_string(),
                            "Priority support".to_string(),
                        ],
                    },
                    PricingTier {
                        name: "Enterprise".to_string(),
                        price: "Custom".to_string(),
                        features: vec![
                            "All Pro features".to_string(),
                            "Dedicated support".to_string(),
                            "Custom integrations".to_string(),
                            "SLA guarantee".to_string(),
                        ],
                    },
                ],
            }),
            AppType::Ecommerce => Some(MonetizationStrategy {
                model: "transaction_fee".to_string(),
                tiers: vec![PricingTier {
                    name: "Standard".to_string(),
                    price: "2.9% + $0.30/transaction".to_string(),
                    features: vec![
                        "Unlimited products".to_string(),
                        "Secure checkout".to_string(),
                        "Inventory management".to_string(),
                    ],
                }],
            }),
            _ => None,
        }
    }
}

impl Default for TemplateProductIntelligence {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProductIntelligence for TemplateProductIntelligence {
    async fn brief(
        &self,
        intent: &Intent,
    ) -> Result<ProductBrief, Box<dyn std::error::Error + Send + Sync>> {
        let (hero_headline, hero_subheadline, cta_text) = self.generate_hero(intent);

        let features: Vec<Feature> = intent
            .explicit
            .extracted_features
            .iter()
            .enumerate()
            .map(|(i, f)| Feature {
                name: f.value.clone(),
                description: format!("{} — inferred from your description", f.value),
                priority: (i + 1) as u32,
            })
            .collect();

        let personas = self.generate_personas(intent);
        let monetization = self.generate_monetization(intent);

        Ok(ProductBrief {
            domain: intent.inferred.domain.value.clone(),
            hero_headline,
            hero_subheadline,
            cta_text,
            features,
            personas,
            monetization,
        })
    }
}
