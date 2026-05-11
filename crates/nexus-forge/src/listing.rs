use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeListing {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub long_description: String,
    pub category: ForgeCategory,
    pub listing_type: ListingType,
    pub author: Author,
    pub version: String,
    pub license: String,
    pub tags: Vec<String>,
    pub downloads: u64,
    pub rating: f64,
    pub review_count: u32,
    pub verified: bool,
    pub revenue_share: Option<RevenueShare>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ForgeCategory {
    Agent,
    Tool,
    Template,
    Workflow,
    Integration,
    Plugin,
    Dataset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ListingType {
    Free,
    Paid { price_cents: u64 },
    Freemium {
        free_tier: String,
        paid_price_cents: u64,
    },
    RevenueShare,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub url: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueShare {
    pub creator_percent: f64,
    pub platform_percent: f64,
    pub min_payout_cents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub listing_id: String,
    pub author_id: String,
    pub rating: u8,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}
