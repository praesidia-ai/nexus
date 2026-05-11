//! Credit system — tracks per-tenant credit balance, usage, and billing plans.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditAccount {
    pub id: String,
    pub tenant_id: String,
    pub plan: String,
    pub credits_total: i32,
    pub credits_used: i32,
    pub credits_reset_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditUsage {
    pub id: String,
    pub account_id: String,
    pub credits_spent: i32,
    pub action_type: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub created_at: String,
}

pub struct CreditService<'c> {
    conn: &'c Connection,
}

impl<'c> CreditService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Returns the credit account for a tenant, creating one with defaults if it does not exist.
    pub fn get_or_create_account(&self, tenant_id: &str) -> Result<CreditAccount> {
        // Try to fetch existing account first.
        let existing = self.conn.query_row(
            "SELECT id, tenant_id, plan, credits_total, credits_used, credits_reset_at, created_at, updated_at
             FROM credit_accounts WHERE tenant_id = ?1",
            params![tenant_id],
            Self::row_to_account,
        );

        match existing {
            Ok(account) => Ok(account),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let id = Uuid::new_v4().to_string();
                self.conn.execute(
                    "INSERT INTO credit_accounts (id, tenant_id) VALUES (?1, ?2)",
                    params![id, tenant_id],
                )?;
                // Re-read to pick up DEFAULT values set by SQLite.
                self.conn
                    .query_row(
                        "SELECT id, tenant_id, plan, credits_total, credits_used, credits_reset_at, created_at, updated_at
                         FROM credit_accounts WHERE id = ?1",
                        params![id],
                        Self::row_to_account,
                    )
                    .map_err(Into::into)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Returns `true` if the tenant has at least `cost` credits remaining.
    pub fn check_credits(&self, tenant_id: &str, cost: i32) -> Result<bool> {
        let account = self.get_or_create_account(tenant_id)?;
        Ok(account.credits_total - account.credits_used >= cost)
    }

    /// Deducts `credits` atomically. Returns `Ok(None)` if the balance is
    /// insufficient (no rows were updated — the caller overdrew) or
    /// `Ok(Some(account))` on success. The atomicity matters because a naive
    /// read-then-write pattern lets two concurrent requests both pass a
    /// balance check and then deduct twice.
    pub fn try_spend_credits(
        &self,
        tenant_id: &str,
        credits: i32,
        action_type: &str,
        description: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Option<CreditAccount>> {
        if credits <= 0 {
            return Ok(Some(self.get_or_create_account(tenant_id)?));
        }
        let account = self.get_or_create_account(tenant_id)?;

        // Conditional update: only succeed if enough balance remains after
        // adding `credits` to `credits_used`. `changes()` returns 0 when the
        // predicate fails, which is how we detect insufficient credit.
        let changed = self.conn.execute(
            "UPDATE credit_accounts
               SET credits_used = credits_used + ?1,
                   updated_at = datetime('now')
             WHERE id = ?2
               AND (credits_total - credits_used) >= ?1",
            params![credits, account.id],
        )?;
        if changed == 0 {
            return Ok(None);
        }

        let usage_id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO credit_usage (id, account_id, credits_spent, action_type, description, project_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![usage_id, account.id, credits, action_type, description, project_id],
        )?;

        Ok(Some(self.get_or_create_account(tenant_id)?))
    }

    /// Legacy non-atomic spend. Kept for callers that intentionally allow
    /// overdraft for post-hoc reconciliation. New code should prefer
    /// [`try_spend_credits`] to enforce atomicity.
    pub fn spend_credits(
        &self,
        tenant_id: &str,
        credits: i32,
        action_type: &str,
        description: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<CreditAccount> {
        let account = self.get_or_create_account(tenant_id)?;
        self.conn.execute(
            "UPDATE credit_accounts SET credits_used = credits_used + ?1, updated_at = datetime('now') WHERE id = ?2",
            params![credits, account.id],
        )?;
        let usage_id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO credit_usage (id, account_id, credits_spent, action_type, description, project_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![usage_id, account.id, credits, action_type, description, project_id],
        )?;
        self.get_or_create_account(tenant_id)
    }

    /// Adds credits to the tenant (for upgrades, top-ups, promotions).
    pub fn add_credits(&self, tenant_id: &str, credits: i32) -> Result<CreditAccount> {
        let account = self.get_or_create_account(tenant_id)?;
        self.conn.execute(
            "UPDATE credit_accounts SET credits_total = credits_total + ?1, updated_at = datetime('now') WHERE id = ?2",
            params![credits, account.id],
        )?;
        self.get_or_create_account(tenant_id)
    }

    /// Returns the most recent usage entries for a tenant.
    pub fn get_usage(&self, tenant_id: &str, limit: usize) -> Result<Vec<CreditUsage>> {
        let account = self.get_or_create_account(tenant_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT id, account_id, credits_spent, action_type, description, project_id, created_at
             FROM credit_usage WHERE account_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![account.id, limit as i64], Self::row_to_usage)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Resets monthly credits for a tenant — zeroes out `credits_used` and bumps the reset date.
    pub fn reset_monthly(&self, tenant_id: &str) -> Result<()> {
        let account = self.get_or_create_account(tenant_id)?;
        self.conn.execute(
            "UPDATE credit_accounts SET credits_used = 0, credits_reset_at = datetime('now', '+30 days'), updated_at = datetime('now') WHERE id = ?1",
            params![account.id],
        )?;
        Ok(())
    }

    fn row_to_account(r: &rusqlite::Row) -> rusqlite::Result<CreditAccount> {
        Ok(CreditAccount {
            id: r.get(0)?,
            tenant_id: r.get(1)?,
            plan: r.get(2)?,
            credits_total: r.get(3)?,
            credits_used: r.get(4)?,
            credits_reset_at: r.get(5)?,
            created_at: r.get(6)?,
            updated_at: r.get(7)?,
        })
    }

    fn row_to_usage(r: &rusqlite::Row) -> rusqlite::Result<CreditUsage> {
        Ok(CreditUsage {
            id: r.get(0)?,
            account_id: r.get(1)?,
            credits_spent: r.get(2)?,
            action_type: r.get(3)?,
            description: r.get(4)?,
            project_id: r.get(5)?,
            created_at: r.get(6)?,
        })
    }
}
