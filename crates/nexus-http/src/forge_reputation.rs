//! Free community reputation for Nexus Forge.
//!
//! Replaces the (now removed) Stripe-based marketplace billing
//! module. Nexus Forge stays **entirely free and open-source**; the
//! marketplace's job is to surface what the community actually trusts
//! via three signals:
//!
//!   * installs (per user, deduped)
//!   * reviews + ratings (1..5, one per reviewer per listing)
//!   * trending score (installs_7d * avg_rating, cheap to rank)
//!
//! The SQL lives in `005_marketplace_reputation.sql`. This module is
//! the pure in-process helpers — ranking math + payload validation —
//! so HTTP handlers and the CLI can share them without either knowing
//! SQL.

use serde::{Deserialize, Serialize};

/// A listing-level stats bundle as served from `forge_stats` (or
/// computed on-the-fly before the row exists).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct ListingStats {
    pub install_count: i64,
    pub install_count_7d: i64,
    pub review_count: i64,
    pub rating_sum: i64,
    pub avg_rating: f64,
}

impl ListingStats {
    /// Trending rank — `installs_7d` gets the primary signal, ratings
    /// act as a multiplier. Unrated listings fall back to a neutral
    /// 3.0 so a brand-new plugin with traction doesn't get buried
    /// below older low-rated ones.
    pub fn trending_score(&self) -> f64 {
        let rating = if self.review_count == 0 {
            3.0
        } else {
            self.avg_rating.clamp(1.0, 5.0)
        };
        (self.install_count_7d as f64) * rating
    }

    /// Confidence-adjusted rating used for the "best-rated" surface.
    /// Wilson lower bound would be overkill here — we use a Bayesian
    /// prior of m=5 neutral (3.0) reviews. A plugin with one perfect
    /// review doesn't outrank a plugin with fifty 4.8-averaged ones.
    pub fn bayesian_rating(&self) -> f64 {
        const PRIOR_N: f64 = 5.0;
        const PRIOR_MEAN: f64 = 3.0;
        let n = self.review_count as f64;
        let sum = self.rating_sum as f64;
        (PRIOR_N * PRIOR_MEAN + sum) / (PRIOR_N + n)
    }

    /// Apply a new review to the in-memory stats. The DB side of this
    /// is an UPSERT; the helper keeps the arithmetic test-covered
    /// without a sqlite harness.
    pub fn apply_review(&mut self, previous: Option<i64>, new_rating: i64) {
        debug_assert!((1..=5).contains(&new_rating));
        match previous {
            None => {
                self.review_count += 1;
                self.rating_sum += new_rating;
            }
            Some(old) => {
                // Existing reviewer changed their rating — review
                // count stays flat, sum shifts by the delta.
                self.rating_sum += new_rating - old;
            }
        }
        self.recompute_avg();
    }

    pub fn apply_install(&mut self) {
        self.install_count += 1;
        // 7-day window is maintained by a scheduled job (not this
        // helper); we only bump the lifetime counter here.
    }

    fn recompute_avg(&mut self) {
        self.avg_rating = if self.review_count == 0 {
            0.0
        } else {
            self.rating_sum as f64 / self.review_count as f64
        };
    }
}

/// Clamp + validate a rating coming off the wire. Returns a sharp
/// error so the handler surfaces a 400, not a 500.
pub fn validate_rating(rating: i64) -> Result<i64, &'static str> {
    if !(1..=5).contains(&rating) {
        return Err("rating must be between 1 and 5");
    }
    Ok(rating)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trending_score_prefers_installs_and_rating() {
        let hot = ListingStats {
            install_count: 500,
            install_count_7d: 120,
            review_count: 40,
            rating_sum: 190,
            avg_rating: 4.75,
        };
        let cold = ListingStats {
            install_count: 10_000,
            install_count_7d: 3,
            review_count: 50,
            rating_sum: 250,
            avg_rating: 5.0,
        };
        assert!(hot.trending_score() > cold.trending_score());
    }

    #[test]
    fn trending_uses_neutral_rating_when_no_reviews() {
        let stats = ListingStats {
            install_count_7d: 10,
            ..Default::default()
        };
        assert!((stats.trending_score() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn bayesian_rating_pulls_single_perfect_toward_prior() {
        let new = ListingStats {
            review_count: 1,
            rating_sum: 5,
            avg_rating: 5.0,
            ..Default::default()
        };
        let mature = ListingStats {
            review_count: 50,
            rating_sum: 240,
            avg_rating: 4.8,
            ..Default::default()
        };
        assert!(mature.bayesian_rating() > new.bayesian_rating());
    }

    #[test]
    fn apply_review_tracks_average() {
        let mut s = ListingStats::default();
        s.apply_review(None, 5);
        s.apply_review(None, 3);
        assert_eq!(s.review_count, 2);
        assert_eq!(s.rating_sum, 8);
        assert!((s.avg_rating - 4.0).abs() < 1e-9);
    }

    #[test]
    fn apply_review_handles_reviewer_updating_their_rating() {
        let mut s = ListingStats::default();
        s.apply_review(None, 2);
        s.apply_review(Some(2), 5);
        assert_eq!(s.review_count, 1);
        assert_eq!(s.rating_sum, 5);
    }

    #[test]
    fn validate_rating_rejects_out_of_range() {
        assert!(validate_rating(0).is_err());
        assert!(validate_rating(6).is_err());
        assert_eq!(validate_rating(4).unwrap(), 4);
    }

    #[test]
    fn apply_install_bumps_lifetime_only() {
        let mut s = ListingStats::default();
        s.apply_install();
        s.apply_install();
        assert_eq!(s.install_count, 2);
        assert_eq!(s.install_count_7d, 0, "7d window maintained elsewhere");
    }
}
