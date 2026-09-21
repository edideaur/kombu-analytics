-- Migration 02: Add bot detection, client error tracking, and performance indices
ALTER TABLE "website_event" ADD COLUMN IF NOT EXISTS "is_bot" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "website_event" ADD COLUMN IF NOT EXISTS "bot_score" SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE "session" ADD COLUMN IF NOT EXISTS "is_bot" BOOLEAN NOT NULL DEFAULT false;

-- Index for bot filtering
CREATE INDEX IF NOT EXISTS "website_event_website_id_is_bot_created_at_idx" ON "website_event"("website_id", "is_bot", "created_at");
CREATE INDEX IF NOT EXISTS "session_website_id_is_bot_created_at_idx" ON "session"("website_id", "is_bot", "created_at");

-- Performance indexes for Web Vitals quantile and breakdown queries
CREATE INDEX IF NOT EXISTS "website_event_website_id_metric_lcp_idx" ON "website_event"("website_id", "created_at") WHERE "lcp" IS NOT NULL;
CREATE INDEX IF NOT EXISTS "website_event_website_id_metric_inp_idx" ON "website_event"("website_id", "created_at") WHERE "inp" IS NOT NULL;
CREATE INDEX IF NOT EXISTS "website_event_website_id_metric_cls_idx" ON "website_event"("website_id", "created_at") WHERE "cls" IS NOT NULL;
CREATE INDEX IF NOT EXISTS "website_event_website_id_metric_fcp_idx" ON "website_event"("website_id", "created_at") WHERE "fcp" IS NOT NULL;
CREATE INDEX IF NOT EXISTS "website_event_website_id_metric_ttfb_idx" ON "website_event"("website_id", "created_at") WHERE "ttfb" IS NOT NULL;

-- Index for client error event querying
CREATE INDEX IF NOT EXISTS "website_event_website_id_event_type_created_at_idx" ON "website_event"("website_id", "event_type", "created_at");

