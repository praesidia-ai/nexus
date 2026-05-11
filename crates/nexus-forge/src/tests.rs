use chrono::Utc;

use crate::listing::*;
use crate::publisher::Publisher;
use crate::search;
use crate::store::ForgeStore;

fn make_author(id: &str) -> Author {
    Author {
        id: id.to_string(),
        name: format!("Author {id}"),
        email: Some(format!("{id}@example.com")),
        url: None,
        verified: true,
    }
}

fn make_listing(id: &str, name: &str, category: ForgeCategory) -> ForgeListing {
    ForgeListing {
        id: id.to_string(),
        name: name.to_string(),
        display_name: format!("Display {name}"),
        description: format!("Description for {name}"),
        long_description: String::new(),
        category,
        listing_type: ListingType::Free,
        author: make_author("author-1"),
        version: "1.0.0".to_string(),
        license: "MIT".to_string(),
        tags: vec!["ai".to_string(), "agent".to_string()],
        downloads: 0,
        rating: 0.0,
        review_count: 0,
        verified: false,
        revenue_share: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        manifest_hash: "abc123hash".to_string(),
    }
}

fn temp_store() -> (tempfile::TempDir, ForgeStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ForgeStore::new(&dir.path().join("forge.db")).unwrap();
    (dir, store)
}

// ---------------------------------------------------------------------------
// Additional store tests (complement inline tests)
// ---------------------------------------------------------------------------

#[test]
fn store_list_by_author() {
    let (_dir, store) = temp_store();
    let mut listing1 = make_listing("la1", "by-alice", ForgeCategory::Tool);
    listing1.author = make_author("alice");
    store.create_listing(&listing1).unwrap();

    let mut listing2 = make_listing("la2", "by-bob", ForgeCategory::Tool);
    listing2.author = make_author("bob");
    store.create_listing(&listing2).unwrap();

    let alice_listings = store.list_by_author("alice", 10);
    assert_eq!(alice_listings.len(), 1);
    assert_eq!(alice_listings[0].author.id, "alice");
}

#[test]
fn store_get_reviews() {
    let (_dir, store) = temp_store();
    store
        .create_listing(&make_listing("rv1", "reviewable", ForgeCategory::Agent))
        .unwrap();

    let review1 = Review {
        id: "r1".to_string(),
        listing_id: "rv1".to_string(),
        author_id: "user-1".to_string(),
        rating: 5,
        title: "Great!".to_string(),
        body: "Works perfectly".to_string(),
        created_at: Utc::now(),
    };
    store.submit_review(&review1).unwrap();

    let review2 = Review {
        id: "r2".to_string(),
        listing_id: "rv1".to_string(),
        author_id: "user-2".to_string(),
        rating: 3,
        title: "Decent".to_string(),
        body: "Could be better".to_string(),
        created_at: Utc::now(),
    };
    store.submit_review(&review2).unwrap();

    let reviews = store.get_reviews("rv1");
    assert_eq!(reviews.len(), 2);
}

#[test]
fn store_list_trending() {
    let (_dir, store) = temp_store();
    store
        .create_listing(&make_listing("t1", "popular", ForgeCategory::Agent))
        .unwrap();
    store
        .create_listing(&make_listing("t2", "unpopular", ForgeCategory::Tool))
        .unwrap();

    for _ in 0..10 {
        store.increment_downloads("t1").unwrap();
    }
    for _ in 0..2 {
        store.increment_downloads("t2").unwrap();
    }

    let trending = store.list_trending(7, 10);
    assert!(!trending.is_empty());
    assert_eq!(trending[0].id, "t1");
}

// ---------------------------------------------------------------------------
// Search tests
// ---------------------------------------------------------------------------

#[test]
fn search_filter_by_category() {
    let (_dir, store) = temp_store();
    store
        .create_listing(&make_listing("sf1", "agent-one", ForgeCategory::Agent))
        .unwrap();
    store
        .create_listing(&make_listing("sf2", "tool-one", ForgeCategory::Tool))
        .unwrap();

    let all = store.search("one", 10);
    assert_eq!(all.len(), 2);

    let agents = store.list_by_category(&ForgeCategory::Agent, 10);
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].category, ForgeCategory::Agent);
}

#[test]
fn search_recommended_empty_history() {
    let (_dir, store) = temp_store();
    store
        .create_listing(&make_listing("rec1", "fallback", ForgeCategory::Agent))
        .unwrap();
    store.increment_downloads("rec1").unwrap();

    let recs = search::recommended(&store, &[]);
    assert!(!recs.is_empty());
}

#[test]
fn search_recommended_with_history() {
    let (_dir, store) = temp_store();
    store
        .create_listing(&make_listing("h1", "liked-agent", ForgeCategory::Agent))
        .unwrap();

    let mut related = make_listing("h2", "same-author-tool", ForgeCategory::Tool);
    related.author = make_author("author-1");
    store.create_listing(&related).unwrap();

    let recs = search::recommended(&store, &["h1".to_string()]);
    assert!(
        recs.iter().any(|l| l.id == "h2"),
        "Should recommend listings from same author"
    );
}

// ---------------------------------------------------------------------------
// Publisher tests
// ---------------------------------------------------------------------------

#[test]
fn publisher_validate_valid_listing() {
    let listing = make_listing("v1", "valid-name", ForgeCategory::Agent);
    assert!(Publisher::validate(&listing).is_ok());
}

#[test]
fn publisher_compute_hash_deterministic() {
    let h1 = Publisher::compute_hash(b"hello world");
    let h2 = Publisher::compute_hash(b"hello world");
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64);

    let h3 = Publisher::compute_hash(b"different");
    assert_ne!(h1, h3);
}
