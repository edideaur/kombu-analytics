-- Migration 06: Data Retention Policies and Auto-Purge History
CREATE TABLE IF NOT EXISTS "website_retention_policy" (
    "website_id" UUID NOT NULL PRIMARY KEY,
    "retention_days" INT NOT NULL DEFAULT 0,
    "auto_purge_enabled" BOOLEAN NOT NULL DEFAULT false,
    "last_purged_at" TIMESTAMPTZ(6),
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6)
);

CREATE TABLE IF NOT EXISTS "retention_purge_log" (
    "purge_id" UUID NOT NULL PRIMARY KEY,
    "website_id" UUID,
    "purged_events" BIGINT NOT NULL DEFAULT 0,
    "purged_sessions" BIGINT NOT NULL DEFAULT 0,
    "cutoff_date" TIMESTAMPTZ(6) NOT NULL,
    "filter_pattern" VARCHAR(500),
    "status" VARCHAR(50) NOT NULL DEFAULT 'completed',
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS "retention_purge_log_website_id_idx" ON "retention_purge_log"("website_id");
CREATE INDEX IF NOT EXISTS "retention_purge_log_created_at_idx" ON "retention_purge_log"("created_at");
