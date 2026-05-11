//! Marketplace — community plugins, skills, workflows, agents.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceItem {
    pub id: String,
    pub item_type: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub icon: String,
    pub tags: Vec<String>,
    pub config: serde_json::Value,
    pub downloads: i32,
    pub rating: f64,
    pub is_official: bool,
    pub created_at: String,
}

pub struct MarketplaceService<'c> { conn: &'c Connection }

impl<'c> MarketplaceService<'c> {
    pub fn new(conn: &'c Connection) -> Self { Self { conn } }

    #[allow(clippy::too_many_arguments)]
    pub fn add_item(&self, item_type: &str, name: &str, description: &str, author: &str, icon: &str, tags: &[String], config: &serde_json::Value) -> Result<MarketplaceItem> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
        let config_str = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
        self.conn.execute(
            "INSERT INTO marketplace_items (id, item_type, name, description, author, icon, tags, config, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, item_type, name, description, author, icon, tags_json, config_str, now],
        )?;
        Ok(MarketplaceItem { id, item_type: item_type.into(), name: name.into(), description: description.into(),
            author: author.into(), version: "1.0.0".into(), icon: icon.into(), tags: tags.to_vec(),
            config: config.clone(), downloads: 0, rating: 0.0, is_official: false, created_at: now })
    }

    pub fn list_by_type(&self, item_type: &str) -> Result<Vec<MarketplaceItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, item_type, name, description, author, version, icon, tags, config, downloads, rating, is_official, created_at
             FROM marketplace_items WHERE item_type=?1 ORDER BY downloads DESC"
        )?;
        let rows = stmt.query_map(params![item_type], Self::row_to_item)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_all(&self) -> Result<Vec<MarketplaceItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, item_type, name, description, author, version, icon, tags, config, downloads, rating, is_official, created_at
             FROM marketplace_items ORDER BY downloads DESC"
        )?;
        let rows = stmt.query_map([], Self::row_to_item)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn search(&self, query: &str) -> Result<Vec<MarketplaceItem>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT id, item_type, name, description, author, version, icon, tags, config, downloads, rating, is_official, created_at
             FROM marketplace_items WHERE name LIKE ?1 OR description LIKE ?1 OR tags LIKE ?1 ORDER BY downloads DESC"
        )?;
        let rows = stmt.query_map(params![pattern], Self::row_to_item)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn increment_downloads(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE marketplace_items SET downloads=downloads+1 WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM marketplace_items WHERE id=?1", params![id])?;
        Ok(())
    }

    fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<MarketplaceItem> {
        let tags_str: String = r.get(7)?;
        let config_str: String = r.get(8)?;
        Ok(MarketplaceItem {
            id: r.get(0)?, item_type: r.get(1)?, name: r.get(2)?, description: r.get(3)?,
            author: r.get(4)?, version: r.get(5)?, icon: r.get(6)?,
            tags: serde_json::from_str(&tags_str).unwrap_or_default(),
            config: serde_json::from_str(&config_str).unwrap_or_default(),
            downloads: r.get(9)?, rating: r.get(10)?,
            is_official: r.get::<_, i64>(11)? != 0, created_at: r.get(12)?,
        })
    }

    /// Seed official marketplace items
    pub fn seed_official(&self) -> Result<()> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM marketplace_items WHERE is_official=1", [], |r| r.get(0))?;
        if count > 0 { return Ok(()); }

        let items: Vec<(&str, &str, &str, &str, &[&str])> = vec![
            ("integration", "Stripe Payments", "Accept payments with Stripe checkout, subscriptions, and webhooks", "\u{1f512}", &["payments", "billing", "stripe"]),
            ("integration", "Email (Resend)", "Send transactional emails with Resend API", "\u{1f4e7}", &["email", "notifications"]),
            ("integration", "Auth (NextAuth)", "Add authentication with email/password, OAuth, and magic links", "\u{1f510}", &["auth", "login", "security"]),
            ("integration", "Database (Prisma)", "Type-safe ORM with PostgreSQL, MySQL, or SQLite", "\u{1f5c4}\u{fe0f}", &["database", "orm", "prisma"]),
            ("integration", "File Upload (S3)", "Upload files to AWS S3 or compatible storage", "\u{1f4c1}", &["storage", "upload", "s3"]),
            ("workflow", "CI/CD Pipeline", "Build, test, and deploy on every push", "\u{1f504}", &["ci", "cd", "deploy"]),
            ("workflow", "SEO Optimizer", "Analyze and improve SEO across all pages", "\u{1f4c8}", &["seo", "marketing"]),
            ("workflow", "Security Audit", "Weekly automated security scan and report", "\u{1f6e1}\u{fe0f}", &["security", "audit"]),
            ("agent", "Analytics Agent", "Track user behavior and generate insights", "\u{1f4ca}", &["analytics", "data"]),
            ("agent", "Support Bot", "AI-powered customer support with knowledge base", "\u{1f916}", &["support", "chat"]),
            ("agent", "Content Writer", "Generate blog posts, docs, and marketing copy", "\u{270d}\u{fe0f}", &["content", "writing"]),
        ];

        let now = Utc::now().to_rfc3339();
        for (item_type, name, desc, icon, tags) in items {
            let id = Uuid::new_v4().to_string();
            let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
            self.conn.execute(
                "INSERT OR IGNORE INTO marketplace_items (id, item_type, name, description, author, icon, tags, is_official, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'Nexus Team', ?5, ?6, 1, ?7)",
                params![id, item_type, name, desc, icon, tags_json, now],
            )?;
        }
        Ok(())
    }
}
