-- Community reputation for Nexus Forge (NEXUS_MASTER_PLAN §9 — revised).
--
-- Nexus Forge is **free and open-source**. There are no payments, no
-- platform fees, no Stripe, no entitlement JWTs. What the marketplace
-- needs instead is a way to surface what the community actually
-- trusts:
--
--   forge_installs  — one row per recorded install event. Per-user
--                     to de-dupe "this user installed foo 12 times
--                     while debugging."
--
--   forge_reviews   — one rating + optional review per (listing,
--                     reviewer) pair. Reviewers can update their
--                     existing review (UPSERT); ratings are 1..5.
--
--   forge_stats     — denormalised aggregates so the gallery never
--                     has to scan installs/reviews for a trending
--                     sort. Bumped by the handler on each install /
--                     review event.
--
-- None of this requires a merchant of record, a KYC flow, or any
-- dollar-denominated anything. It's GitHub-stars-plus-npm-downloads
-- for Nexus plugins.

CREATE TABLE IF NOT EXISTS forge_installs (
    id               TEXT PRIMARY KEY NOT NULL,
    listing_id       TEXT NOT NULL,
    account_id       TEXT NOT NULL,
    nexus_version    TEXT,
    source           TEXT NOT NULL DEFAULT 'cli', -- cli | web | api | mcp
    created_at       TEXT NOT NULL,
    FOREIGN KEY (listing_id) REFERENCES marketplace_items(id) ON DELETE CASCADE,
    UNIQUE (listing_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_forge_installs_listing
    ON forge_installs(listing_id, created_at DESC);

CREATE TABLE IF NOT EXISTS forge_reviews (
    id              TEXT PRIMARY KEY NOT NULL,
    listing_id      TEXT NOT NULL,
    reviewer_id     TEXT NOT NULL,
    rating          INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 5),
    body            TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    FOREIGN KEY (listing_id) REFERENCES marketplace_items(id) ON DELETE CASCADE,
    UNIQUE (listing_id, reviewer_id)
);

CREATE INDEX IF NOT EXISTS idx_forge_reviews_listing
    ON forge_reviews(listing_id, created_at DESC);

CREATE TABLE IF NOT EXISTS forge_stats (
    listing_id       TEXT PRIMARY KEY NOT NULL,
    install_count    INTEGER NOT NULL DEFAULT 0,
    install_count_7d INTEGER NOT NULL DEFAULT 0,
    review_count     INTEGER NOT NULL DEFAULT 0,
    rating_sum       INTEGER NOT NULL DEFAULT 0,
    /* Derived: rating_sum / review_count when review_count > 0.
       Kept denormalised so the gallery's "trending = installs_7d *
       avg_rating" sort stays an indexed scan. */
    avg_rating       REAL NOT NULL DEFAULT 0.0,
    last_install_at  TEXT,
    FOREIGN KEY (listing_id) REFERENCES marketplace_items(id) ON DELETE CASCADE
);
