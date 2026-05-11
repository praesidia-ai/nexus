//! User preferences — display mode, onboarding state, theme, quality settings.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub tenant_id: String,
    pub display_mode: String,
    pub onboarding_completed: bool,
    pub onboarding_step: i32,
    pub theme: String,
    pub show_cost_estimates: bool,
    pub preferred_quality: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct UserPrefsService<'c> {
    conn: &'c Connection,
}

impl<'c> UserPrefsService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Returns the preferences for a tenant, creating defaults if none exist.
    pub fn get_or_create(&self, tenant_id: &str) -> Result<UserPreferences> {
        let existing = self.conn.query_row(
            "SELECT tenant_id, display_mode, onboarding_completed, onboarding_step, theme, show_cost_estimates, preferred_quality, created_at, updated_at
             FROM user_profile_prefs WHERE tenant_id = ?1",
            params![tenant_id],
            Self::row_to_prefs,
        );

        match existing {
            Ok(prefs) => Ok(prefs),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                self.conn.execute(
                    "INSERT INTO user_profile_prefs (tenant_id) VALUES (?1)",
                    params![tenant_id],
                )?;
                self.conn
                    .query_row(
                        "SELECT tenant_id, display_mode, onboarding_completed, onboarding_step, theme, show_cost_estimates, preferred_quality, created_at, updated_at
                         FROM user_profile_prefs WHERE tenant_id = ?1",
                        params![tenant_id],
                        Self::row_to_prefs,
                    )
                    .map_err(Into::into)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Update display mode: "consumer", "creator", or "developer".
    pub fn update_mode(&self, tenant_id: &str, mode: &str) -> Result<()> {
        self.get_or_create(tenant_id)?; // ensure row exists
        self.conn.execute(
            "UPDATE user_profile_prefs SET display_mode = ?1, updated_at = datetime('now') WHERE tenant_id = ?2",
            params![mode, tenant_id],
        )?;
        Ok(())
    }

    /// Advance onboarding to the given step number.
    pub fn complete_onboarding_step(&self, tenant_id: &str, step: i32) -> Result<()> {
        self.get_or_create(tenant_id)?;
        self.conn.execute(
            "UPDATE user_profile_prefs SET onboarding_step = ?1, updated_at = datetime('now') WHERE tenant_id = ?2",
            params![step, tenant_id],
        )?;
        Ok(())
    }

    /// Mark onboarding as fully complete.
    pub fn mark_onboarding_complete(&self, tenant_id: &str) -> Result<()> {
        self.get_or_create(tenant_id)?;
        self.conn.execute(
            "UPDATE user_profile_prefs SET onboarding_completed = 1, updated_at = datetime('now') WHERE tenant_id = ?1",
            params![tenant_id],
        )?;
        Ok(())
    }

    /// Set preferred quality: "fast", "balanced", or "best".
    pub fn set_preferred_quality(&self, tenant_id: &str, quality: &str) -> Result<()> {
        self.get_or_create(tenant_id)?;
        self.conn.execute(
            "UPDATE user_profile_prefs SET preferred_quality = ?1, updated_at = datetime('now') WHERE tenant_id = ?2",
            params![quality, tenant_id],
        )?;
        Ok(())
    }

    /// Returns just the display mode string for a tenant.
    pub fn get_display_mode(&self, tenant_id: &str) -> Result<String> {
        let prefs = self.get_or_create(tenant_id)?;
        Ok(prefs.display_mode)
    }

    fn row_to_prefs(r: &rusqlite::Row) -> rusqlite::Result<UserPreferences> {
        Ok(UserPreferences {
            tenant_id: r.get(0)?,
            display_mode: r.get(1)?,
            onboarding_completed: r.get::<_, i64>(2)? != 0,
            onboarding_step: r.get(3)?,
            theme: r.get(4)?,
            show_cost_estimates: r.get::<_, i64>(5)? != 0,
            preferred_quality: r.get(6)?,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
        })
    }
}
