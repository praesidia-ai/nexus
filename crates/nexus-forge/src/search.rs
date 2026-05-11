use crate::listing::{ForgeCategory, ForgeListing, ListingType};
use crate::store::ForgeStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilter {
    pub query: Option<String>,
    pub category: Option<ForgeCategory>,
    pub listing_type: Option<ListingType>,
    pub min_rating: Option<f64>,
    pub verified_only: bool,
    pub tags: Vec<String>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub items: Vec<ForgeListing>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

pub fn filter_listings(listings: &[ForgeListing], filter: &SearchFilter) -> SearchResults {
    let mut filtered: Vec<&ForgeListing> = listings.iter().collect();

    if let Some(ref q) = filter.query {
        let q_lower = q.to_lowercase();
        filtered.retain(|l| {
            l.name.to_lowercase().contains(&q_lower)
                || l.description.to_lowercase().contains(&q_lower)
                || l.tags.iter().any(|t| t.to_lowercase().contains(&q_lower))
        });
    }

    if let Some(ref cat) = filter.category {
        filtered.retain(|l| &l.category == cat);
    }

    if let Some(min) = filter.min_rating {
        filtered.retain(|l| l.rating >= min);
    }

    if filter.verified_only {
        filtered.retain(|l| l.verified);
    }

    if !filter.tags.is_empty() {
        filtered.retain(|l| {
            filter
                .tags
                .iter()
                .any(|ft| l.tags.iter().any(|lt| lt == ft))
        });
    }

    let total = filtered.len();
    let limit = if filter.limit == 0 { 20 } else { filter.limit };
    let items: Vec<ForgeListing> = filtered
        .into_iter()
        .skip(filter.offset)
        .take(limit)
        .cloned()
        .collect();

    SearchResults {
        items,
        total,
        limit,
        offset: filter.offset,
    }
}

pub fn trending(listings: &[ForgeListing], limit: usize) -> Vec<ForgeListing> {
    let mut sorted: Vec<ForgeListing> = listings.to_vec();
    sorted.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    sorted.truncate(limit);
    sorted
}

/// Simple collaborative filtering: given a user's download history (listing IDs),
/// find other listings by the same authors or in the same categories.
pub fn recommended(
    store: &ForgeStore,
    user_history: &[String],
) -> Vec<ForgeListing> {
    if user_history.is_empty() {
        return store.list_trending(30, 10);
    }

    let mut author_ids = Vec::new();
    let mut categories = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> =
        user_history.iter().cloned().collect();

    for id in user_history {
        if let Ok(listing) = store.get_listing(id) {
            if !author_ids.contains(&listing.author.id) {
                author_ids.push(listing.author.id.clone());
            }
            if !categories.contains(&listing.category) {
                categories.push(listing.category.clone());
            }
        }
    }

    let mut recommendations = Vec::new();

    for author_id in &author_ids {
        for l in store.list_by_author(author_id, 20) {
            if !seen_ids.contains(&l.id) {
                seen_ids.insert(l.id.clone());
                recommendations.push(l);
            }
        }
    }

    for cat in &categories {
        for l in store.list_by_category(cat, 20) {
            if !seen_ids.contains(&l.id) {
                seen_ids.insert(l.id.clone());
                recommendations.push(l);
            }
        }
    }

    recommendations.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    recommendations.truncate(20);
    recommendations
}
