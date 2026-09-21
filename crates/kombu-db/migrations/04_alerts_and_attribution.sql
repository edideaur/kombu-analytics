-- Migration 04: Alerting, Webhooks, and Performance Budgets
CREATE TABLE IF NOT EXISTS "alert_rule" (
    "alert_id" UUID NOT NULL PRIMARY KEY,
    "website_id" UUID NOT NULL,
    "name" VARCHAR(100) NOT NULL,
    "alert_type" VARCHAR(50) NOT NULL,
    "webhook_url" VARCHAR(1000) NOT NULL,
    "channel_type" VARCHAR(50) NOT NULL DEFAULT 'generic',
    "threshold" DOUBLE PRECISION NOT NULL,
    "comparison" VARCHAR(10) NOT NULL DEFAULT 'gt',
    "metric" VARCHAR(50),
    "url_path" VARCHAR(500),
    "window_minutes" INT NOT NULL DEFAULT 60,
    "enabled" BOOLEAN NOT NULL DEFAULT true,
    "last_triggered_at" TIMESTAMPTZ(6),
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6)
);

CREATE INDEX IF NOT EXISTS "alert_rule_website_id_idx" ON "alert_rule"("website_id");
CREATE INDEX IF NOT EXISTS "alert_rule_enabled_idx" ON "alert_rule"("enabled");

CREATE TABLE IF NOT EXISTS "alert_history" (
    "history_id" UUID NOT NULL PRIMARY KEY,
    "alert_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "alert_name" VARCHAR(100) NOT NULL,
    "triggered_value" DOUBLE PRECISION NOT NULL,
    "threshold" DOUBLE PRECISION NOT NULL,
    "message" TEXT NOT NULL,
    "status" VARCHAR(20) NOT NULL DEFAULT 'sent',
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS "alert_history_alert_id_idx" ON "alert_history"("alert_id");
CREATE INDEX IF NOT EXISTS "alert_history_website_id_idx" ON "alert_history"("website_id");
CREATE INDEX IF NOT EXISTS "alert_history_created_at_idx" ON "alert_history"("created_at");
