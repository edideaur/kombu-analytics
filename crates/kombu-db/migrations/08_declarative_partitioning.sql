-- 08_declarative_partitioning.sql
-- PostgreSQL Declarative Partitioning & Rollup Tables

-- Rollup table for declarative partitioning & high scale hourly rollups
CREATE TABLE IF NOT EXISTS "website_event_stats_hourly" (
    "website_id" UUID NOT NULL,
    "hour_bucket" TIMESTAMPTZ(6) NOT NULL,
    "views" BIGINT NOT NULL DEFAULT 0,
    "visitors" BIGINT NOT NULL DEFAULT 0,
    "visits" BIGINT NOT NULL DEFAULT 0,
    "bounces" BIGINT NOT NULL DEFAULT 0,
    "totaltime" BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT "website_event_stats_hourly_pkey" PRIMARY KEY ("website_id", "hour_bucket")
);

CREATE INDEX IF NOT EXISTS "website_event_stats_hourly_idx"
    ON "website_event_stats_hourly"("website_id", "hour_bucket");

-- Metadata registry for partition management
CREATE TABLE IF NOT EXISTS "analytics_partition_registry" (
    "table_name" VARCHAR(100) PRIMARY KEY,
    "parent_table" VARCHAR(100) NOT NULL,
    "partition_key" VARCHAR(50) NOT NULL,
    "range_start" TIMESTAMPTZ(6) NOT NULL,
    "range_end" TIMESTAMPTZ(6) NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Function to refresh hourly rollups for a given website and time window
CREATE OR REPLACE FUNCTION refresh_website_event_stats_hourly(
    p_website_id UUID,
    p_start TIMESTAMPTZ,
    p_end TIMESTAMPTZ
) RETURNS VOID AS $$
BEGIN
    INSERT INTO "website_event_stats_hourly" (
        "website_id", "hour_bucket", "views", "visitors", "visits", "bounces", "totaltime"
    )
    SELECT
        we.website_id,
        date_trunc('hour', we.created_at) AS hour_bucket,
        COUNT(*) FILTER (WHERE we.event_type NOT IN (2, 5)) AS views,
        COUNT(DISTINCT we.session_id) AS visitors,
        COUNT(DISTINCT we.visit_id) AS visits,
        COUNT(DISTINCT we.visit_id) FILTER (WHERE sub.event_count = 1 AND sub.has_custom = 0) AS bounces,
        COALESCE(SUM(EXTRACT(EPOCH FROM (sub.max_time - sub.min_time))), 0)::bigint AS totaltime
    FROM "website_event" we
    JOIN (
        SELECT
            visit_id,
            COUNT(*) AS event_count,
            MAX(CASE WHEN event_type = 2 THEN 1 ELSE 0 END) AS has_custom,
            MIN(created_at) AS min_time,
            MAX(created_at) AS max_time
        FROM "website_event"
        WHERE website_id = p_website_id
          AND created_at >= p_start AND created_at <= p_end
          AND is_bot = false
        GROUP BY visit_id
    ) sub ON we.visit_id = sub.visit_id
    WHERE we.website_id = p_website_id
      AND we.created_at >= p_start AND we.created_at <= p_end
      AND we.is_bot = false
    GROUP BY we.website_id, date_trunc('hour', we.created_at)
    ON CONFLICT ("website_id", "hour_bucket")
    DO UPDATE SET
        views = EXCLUDED.views,
        visitors = EXCLUDED.visitors,
        visits = EXCLUDED.visits,
        bounces = EXCLUDED.bounces,
        totaltime = EXCLUDED.totaltime;
END;
$$ LANGUAGE plpgsql;
