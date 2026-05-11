use crate::error::{ForgeError, ForgeResult};
use crate::listing::*;
use chrono::Utc;
use rusqlite::Connection;
use tracing::info;

pub struct ForgeStore {
    db: Connection,
}

impl ForgeStore {
    pub fn new(db_path: &std::path::Path) -> ForgeResult<Self> {
        let db = Connection::open(db_path)?;
        db.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;

            CREATE TABLE IF NOT EXISTS forge_listings (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL,
                long_description TEXT NOT NULL DEFAULT '',
                category TEXT NOT NULL,
                listing_type TEXT NOT NULL,
                author_id TEXT NOT NULL,
                author_name TEXT NOT NULL,
                author_verified INTEGER NOT NULL DEFAULT 0,
                version TEXT NOT NULL,
                license TEXT NOT NULL DEFAULT 'MIT',
                tags TEXT NOT NULL DEFAULT '[]',
                downloads INTEGER NOT NULL DEFAULT 0,
                rating REAL NOT NULL DEFAULT 0.0,
                review_count INTEGER NOT NULL DEFAULT 0,
                verified INTEGER NOT NULL DEFAULT 0,
                revenue_share TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                manifest_hash TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_fl_category ON forge_listings(category);
            CREATE INDEX IF NOT EXISTS idx_fl_author ON forge_listings(author_id);
            CREATE INDEX IF NOT EXISTS idx_fl_downloads ON forge_listings(downloads DESC);
            CREATE INDEX IF NOT EXISTS idx_fl_rating ON forge_listings(rating DESC);

            CREATE TABLE IF NOT EXISTS forge_reviews (
                id TEXT PRIMARY KEY,
                listing_id TEXT NOT NULL,
                author_id TEXT NOT NULL,
                rating INTEGER NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (listing_id) REFERENCES forge_listings(id)
            );

            CREATE INDEX IF NOT EXISTS idx_fr_listing ON forge_reviews(listing_id);

            CREATE TABLE IF NOT EXISTS forge_download_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                listing_id TEXT NOT NULL,
                downloaded_at TEXT NOT NULL,
                FOREIGN KEY (listing_id) REFERENCES forge_listings(id)
            );

            CREATE INDEX IF NOT EXISTS idx_fde_listing ON forge_download_events(listing_id);
            CREATE INDEX IF NOT EXISTS idx_fde_date ON forge_download_events(downloaded_at);
        ",
        )?;
        info!("Forge store initialised");
        Ok(Self { db })
    }

    pub fn create_listing(&self, listing: &ForgeListing) -> ForgeResult<()> {
        let category = serde_json::to_string(&listing.category).unwrap_or_default();
        let listing_type = serde_json::to_string(&listing.listing_type).unwrap_or_default();
        let tags = serde_json::to_string(&listing.tags).unwrap_or_default();
        let revenue = listing
            .revenue_share
            .as_ref()
            .map(|r| serde_json::to_string(r).unwrap_or_default());

        self.db.execute(
            "INSERT INTO forge_listings (id, name, display_name, description, long_description, category, listing_type, author_id, author_name, author_verified, version, license, tags, downloads, rating, review_count, verified, revenue_share, created_at, updated_at, manifest_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            rusqlite::params![
                listing.id,
                listing.name,
                listing.display_name,
                listing.description,
                listing.long_description,
                category.trim_matches('"'),
                listing_type,
                listing.author.id,
                listing.author.name,
                listing.author.verified as i32,
                listing.version,
                listing.license,
                tags,
                listing.downloads as i64,
                listing.rating,
                listing.review_count,
                listing.verified as i32,
                revenue,
                listing.created_at.to_rfc3339(),
                listing.updated_at.to_rfc3339(),
                listing.manifest_hash,
            ],
        )?;
        Ok(())
    }

    pub fn get_listing(&self, id: &str) -> ForgeResult<ForgeListing> {
        self.db
            .query_row(
                "SELECT id, name, display_name, description, long_description, category, listing_type, author_id, author_name, author_verified, version, license, tags, downloads, rating, review_count, verified, revenue_share, created_at, updated_at, manifest_hash
             FROM forge_listings WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok(row_to_listing(row)),
            )
            .map_err(|_| ForgeError::NotFound(format!("listing {id}")))
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<ForgeListing> {
        let pattern = format!("%{query}%");
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT id, name, display_name, description, long_description, category, listing_type, author_id, author_name, author_verified, version, license, tags, downloads, rating, review_count, verified, revenue_share, created_at, updated_at, manifest_hash
             FROM forge_listings WHERE name LIKE ?1 OR description LIKE ?1 OR tags LIKE ?1
             ORDER BY downloads DESC LIMIT ?2",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![pattern, limit as i64], |row| {
                    Ok(row_to_listing(row))
                })
                .map(|rows| {
                    for listing in rows.flatten() {
                        results.push(listing);
                    }
                });
        }
        results
    }

    pub fn list_by_category(&self, category: &ForgeCategory, limit: usize) -> Vec<ForgeListing> {
        let cat = serde_json::to_string(category).unwrap_or_default();
        let cat_clean = cat.trim_matches('"');
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT id, name, display_name, description, long_description, category, listing_type, author_id, author_name, author_verified, version, license, tags, downloads, rating, review_count, verified, revenue_share, created_at, updated_at, manifest_hash
             FROM forge_listings WHERE category = ?1 ORDER BY downloads DESC LIMIT ?2",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![cat_clean, limit as i64], |row| {
                    Ok(row_to_listing(row))
                })
                .map(|rows| {
                    for listing in rows.flatten() {
                        results.push(listing);
                    }
                });
        }
        results
    }

    pub fn list_by_author(&self, author_id: &str, limit: usize) -> Vec<ForgeListing> {
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT id, name, display_name, description, long_description, category, listing_type, author_id, author_name, author_verified, version, license, tags, downloads, rating, review_count, verified, revenue_share, created_at, updated_at, manifest_hash
             FROM forge_listings WHERE author_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![author_id, limit as i64], |row| {
                    Ok(row_to_listing(row))
                })
                .map(|rows| {
                    for listing in rows.flatten() {
                        results.push(listing);
                    }
                });
        }
        results
    }

    pub fn list_trending(&self, days: u32, limit: usize) -> Vec<ForgeListing> {
        let cutoff =
            (Utc::now() - chrono::Duration::days(i64::from(days))).to_rfc3339();
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT l.id, l.name, l.display_name, l.description, l.long_description, l.category, l.listing_type, l.author_id, l.author_name, l.author_verified, l.version, l.license, l.tags, l.downloads, l.rating, l.review_count, l.verified, l.revenue_share, l.created_at, l.updated_at, l.manifest_hash
             FROM forge_listings l
             LEFT JOIN forge_download_events de ON l.id = de.listing_id AND de.downloaded_at >= ?1
             GROUP BY l.id
             ORDER BY COUNT(de.id) DESC
             LIMIT ?2",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![cutoff, limit as i64], |row| {
                    Ok(row_to_listing(row))
                })
                .map(|rows| {
                    for listing in rows.flatten() {
                        results.push(listing);
                    }
                });
        }
        results
    }

    pub fn increment_downloads(&self, id: &str) -> ForgeResult<()> {
        let now = Utc::now().to_rfc3339();
        self.db.execute(
            "INSERT INTO forge_download_events (listing_id, downloaded_at) VALUES (?1, ?2)",
            rusqlite::params![id, now],
        )?;
        self.db.execute(
            "UPDATE forge_listings SET downloads = downloads + 1 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    pub fn submit_review(&self, review: &Review) -> ForgeResult<()> {
        self.db.execute(
            "INSERT INTO forge_reviews (id, listing_id, author_id, rating, title, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                review.id,
                review.listing_id,
                review.author_id,
                review.rating as i32,
                review.title,
                review.body,
                review.created_at.to_rfc3339(),
            ],
        )?;
        self.update_rating(&review.listing_id)?;
        Ok(())
    }

    pub fn get_reviews(&self, listing_id: &str) -> Vec<Review> {
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT id, listing_id, author_id, rating, title, body, created_at
             FROM forge_reviews WHERE listing_id = ?1 ORDER BY created_at DESC",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![listing_id], |row| {
                    Ok(Review {
                        id: row.get(0)?,
                        listing_id: row.get(1)?,
                        author_id: row.get(2)?,
                        rating: row.get::<_, i32>(3)? as u8,
                        title: row.get(4)?,
                        body: row.get(5)?,
                        created_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(6)?,
                        )
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    })
                })
                .map(|rows| {
                    for review in rows.flatten() {
                        results.push(review);
                    }
                });
        }
        results
    }

    fn update_rating(&self, listing_id: &str) -> ForgeResult<()> {
        self.db.execute(
            "UPDATE forge_listings SET
                rating = (SELECT COALESCE(AVG(rating), 0) FROM forge_reviews WHERE listing_id = ?1),
                review_count = (SELECT COUNT(*) FROM forge_reviews WHERE listing_id = ?1)
             WHERE id = ?1",
            rusqlite::params![listing_id],
        )?;
        Ok(())
    }
}

fn row_to_listing(row: &rusqlite::Row<'_>) -> ForgeListing {
    let category_str: String = row.get(5).unwrap_or_default();
    let category = match category_str.as_str() {
        "agent" => ForgeCategory::Agent,
        "tool" => ForgeCategory::Tool,
        "template" => ForgeCategory::Template,
        "workflow" => ForgeCategory::Workflow,
        "integration" => ForgeCategory::Integration,
        "plugin" => ForgeCategory::Plugin,
        "dataset" => ForgeCategory::Dataset,
        _ => ForgeCategory::Plugin,
    };

    let listing_type_str: String = row.get(6).unwrap_or_default();
    let listing_type =
        serde_json::from_str(&listing_type_str).unwrap_or(ListingType::Free);

    let tags_str: String = row.get(12).unwrap_or_default();
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

    let revenue_str: Option<String> = row.get(17).unwrap_or(None);
    let revenue_share = revenue_str.and_then(|s| serde_json::from_str(&s).ok());

    ForgeListing {
        id: row.get(0).unwrap_or_default(),
        name: row.get(1).unwrap_or_default(),
        display_name: row.get(2).unwrap_or_default(),
        description: row.get(3).unwrap_or_default(),
        long_description: row.get(4).unwrap_or_default(),
        category,
        listing_type,
        author: Author {
            id: row.get(7).unwrap_or_default(),
            name: row.get(8).unwrap_or_default(),
            email: None,
            url: None,
            verified: row.get::<_, i32>(9).unwrap_or(0) != 0,
        },
        version: row.get(10).unwrap_or_default(),
        license: row.get(11).unwrap_or_default(),
        tags,
        downloads: row.get::<_, i64>(13).unwrap_or(0) as u64,
        rating: row.get(14).unwrap_or(0.0),
        review_count: row.get::<_, i32>(15).unwrap_or(0) as u32,
        verified: row.get::<_, i32>(16).unwrap_or(0) != 0,
        revenue_share,
        created_at: chrono::DateTime::parse_from_rfc3339(
            &row.get::<_, String>(18).unwrap_or_default(),
        )
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(
            &row.get::<_, String>(19).unwrap_or_default(),
        )
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now()),
        manifest_hash: row.get(20).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_listing() -> ForgeListing {
        ForgeListing {
            id: uuid::Uuid::new_v4().to_string(),
            name: "test-agent".to_string(),
            display_name: "Test Agent".to_string(),
            description: "A test agent for the Nexus marketplace".to_string(),
            long_description: "Extended description".to_string(),
            category: ForgeCategory::Agent,
            listing_type: ListingType::Free,
            author: Author {
                id: "author-1".to_string(),
                name: "Test Author".to_string(),
                email: None,
                url: None,
                verified: true,
            },
            version: "1.0.0".to_string(),
            license: "MIT".to_string(),
            tags: vec!["test".to_string(), "agent".to_string()],
            downloads: 0,
            rating: 0.0,
            review_count: 0,
            verified: false,
            revenue_share: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            manifest_hash: "abc123".to_string(),
        }
    }

    #[test]
    fn test_create_and_get_listing() {
        let dir = tempdir().unwrap();
        let store = ForgeStore::new(&dir.path().join("forge.db")).unwrap();
        let listing = test_listing();
        store.create_listing(&listing).unwrap();
        let retrieved = store.get_listing(&listing.id).unwrap();
        assert_eq!(retrieved.name, "test-agent");
        assert_eq!(retrieved.display_name, "Test Agent");
    }

    #[test]
    fn test_search() {
        let dir = tempdir().unwrap();
        let store = ForgeStore::new(&dir.path().join("forge.db")).unwrap();
        let listing = test_listing();
        store.create_listing(&listing).unwrap();
        let results = store.search("test", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_downloads_increment() {
        let dir = tempdir().unwrap();
        let store = ForgeStore::new(&dir.path().join("forge.db")).unwrap();
        let listing = test_listing();
        store.create_listing(&listing).unwrap();
        store.increment_downloads(&listing.id).unwrap();
        store.increment_downloads(&listing.id).unwrap();
        let updated = store.get_listing(&listing.id).unwrap();
        assert_eq!(updated.downloads, 2);
    }

    #[test]
    fn test_review_and_rating() {
        let dir = tempdir().unwrap();
        let store = ForgeStore::new(&dir.path().join("forge.db")).unwrap();
        let listing = test_listing();
        store.create_listing(&listing).unwrap();

        let review = Review {
            id: uuid::Uuid::new_v4().to_string(),
            listing_id: listing.id.clone(),
            author_id: "user-1".to_string(),
            rating: 5,
            title: "Great!".to_string(),
            body: "Works perfectly".to_string(),
            created_at: Utc::now(),
        };
        store.submit_review(&review).unwrap();

        let updated = store.get_listing(&listing.id).unwrap();
        assert_eq!(updated.review_count, 1);
        assert!((updated.rating - 5.0).abs() < 0.01);
    }
}
