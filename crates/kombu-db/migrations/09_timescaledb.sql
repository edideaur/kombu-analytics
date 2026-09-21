-- 09_timescaledb.sql
-- TimescaleDB Extension, Hypertables, & Continuous Aggregate Setup

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb') THEN
        CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;
    ELSE
        RAISE NOTICE 'TimescaleDB extension not available in PostgreSQL installation; skipping hypertable creation';
    END IF;
END $$;

-- Registry for tracking TimescaleDB hypertables & continuous aggregates
CREATE TABLE IF NOT EXISTS "timescale_hypertable_registry" (
    "hypertable_name" VARCHAR(100) PRIMARY KEY,
    "time_column" VARCHAR(50) NOT NULL,
    "chunk_interval" VARCHAR(50) NOT NULL DEFAULT '7 days',
    "compression_enabled" BOOLEAN NOT NULL DEFAULT false,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP
);
