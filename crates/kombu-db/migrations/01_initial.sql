-- kombu/umami/prisma/migrations/01_init/migration.sql
-- CreateExtension
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- CreateTable
CREATE TABLE "user" (
    "user_id" UUID NOT NULL,
    "username" VARCHAR(255) NOT NULL,
    "password" VARCHAR(60) NOT NULL,
    "role" VARCHAR(50) NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),
    "deleted_at" TIMESTAMPTZ(6),

    CONSTRAINT "user_pkey" PRIMARY KEY ("user_id")
);

-- CreateTable
CREATE TABLE "session" (
    "session_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "hostname" VARCHAR(100),
    "browser" VARCHAR(20),
    "os" VARCHAR(20),
    "device" VARCHAR(20),
    "screen" VARCHAR(11),
    "language" VARCHAR(35),
    "country" CHAR(2),
    "subdivision1" VARCHAR(20),
    "subdivision2" VARCHAR(50),
    "city" VARCHAR(50),
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "session_pkey" PRIMARY KEY ("session_id")
);

-- CreateTable
CREATE TABLE "website" (
    "website_id" UUID NOT NULL,
    "name" VARCHAR(100) NOT NULL,
    "domain" VARCHAR(500),
    "share_id" VARCHAR(50),
    "reset_at" TIMESTAMPTZ(6),
    "user_id" UUID,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),
    "deleted_at" TIMESTAMPTZ(6),

    CONSTRAINT "website_pkey" PRIMARY KEY ("website_id")
);

-- CreateTable
CREATE TABLE "website_event" (
    "event_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "session_id" UUID NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "url_path" VARCHAR(500) NOT NULL,
    "url_query" VARCHAR(500),
    "referrer_path" VARCHAR(500),
    "referrer_query" VARCHAR(500),
    "referrer_domain" VARCHAR(500),
    "page_title" VARCHAR(500),
    "event_type" INTEGER NOT NULL DEFAULT 1,
    "event_name" VARCHAR(50),

    CONSTRAINT "website_event_pkey" PRIMARY KEY ("event_id")
);

-- CreateTable
CREATE TABLE "event_data" (
    "event_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "website_event_id" UUID NOT NULL,
    "event_key" VARCHAR(500) NOT NULL,
    "event_string_value" VARCHAR(500),
    "event_numeric_value" DECIMAL(19,4),
    "event_date_value" TIMESTAMPTZ(6),
    "event_data_type" INTEGER NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "event_data_pkey" PRIMARY KEY ("event_id")
);

-- CreateTable
CREATE TABLE "team" (
    "team_id" UUID NOT NULL,
    "name" VARCHAR(50) NOT NULL,
    "access_code" VARCHAR(50),
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),

    CONSTRAINT "team_pkey" PRIMARY KEY ("team_id")
);

-- CreateTable
CREATE TABLE "team_user" (
    "team_user_id" UUID NOT NULL,
    "team_id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "role" VARCHAR(50) NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),

    CONSTRAINT "team_user_pkey" PRIMARY KEY ("team_user_id")
);

-- CreateTable
CREATE TABLE "team_website" (
    "team_website_id" UUID NOT NULL,
    "team_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "team_website_pkey" PRIMARY KEY ("team_website_id")
);

-- CreateIndex
CREATE UNIQUE INDEX "user_user_id_key" ON "user"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "user_username_key" ON "user"("username");

-- CreateIndex
CREATE UNIQUE INDEX "session_session_id_key" ON "session"("session_id");

-- CreateIndex
CREATE INDEX "session_created_at_idx" ON "session"("created_at");

-- CreateIndex
CREATE INDEX "session_website_id_idx" ON "session"("website_id");

-- CreateIndex
CREATE UNIQUE INDEX "website_website_id_key" ON "website"("website_id");

-- CreateIndex
CREATE UNIQUE INDEX "website_share_id_key" ON "website"("share_id");

-- CreateIndex
CREATE INDEX "website_user_id_idx" ON "website"("user_id");

-- CreateIndex
CREATE INDEX "website_created_at_idx" ON "website"("created_at");

-- CreateIndex
CREATE INDEX "website_share_id_idx" ON "website"("share_id");

-- CreateIndex
CREATE INDEX "website_event_created_at_idx" ON "website_event"("created_at");

-- CreateIndex
CREATE INDEX "website_event_session_id_idx" ON "website_event"("session_id");

-- CreateIndex
CREATE INDEX "website_event_website_id_idx" ON "website_event"("website_id");

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_idx" ON "website_event"("website_id", "created_at");

-- CreateIndex
CREATE INDEX "website_event_website_id_session_id_created_at_idx" ON "website_event"("website_id", "session_id", "created_at");

-- CreateIndex
CREATE INDEX "event_data_created_at_idx" ON "event_data"("created_at");

-- CreateIndex
CREATE INDEX "event_data_website_id_idx" ON "event_data"("website_id");

-- CreateIndex
CREATE INDEX "event_data_website_event_id_idx" ON "event_data"("website_event_id");

-- CreateIndex
CREATE UNIQUE INDEX "team_team_id_key" ON "team"("team_id");

-- CreateIndex
CREATE UNIQUE INDEX "team_access_code_key" ON "team"("access_code");

-- CreateIndex
CREATE INDEX "team_access_code_idx" ON "team"("access_code");

-- CreateIndex
CREATE UNIQUE INDEX "team_user_team_user_id_key" ON "team_user"("team_user_id");

-- CreateIndex
CREATE INDEX "team_user_team_id_idx" ON "team_user"("team_id");

-- CreateIndex
CREATE INDEX "team_user_user_id_idx" ON "team_user"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "team_website_team_website_id_key" ON "team_website"("team_website_id");

-- CreateIndex
CREATE INDEX "team_website_team_id_idx" ON "team_website"("team_id");

-- CreateIndex
CREATE INDEX "team_website_website_id_idx" ON "team_website"("website_id");

-- AddSystemUser
INSERT INTO "user" (user_id, username, role, password) VALUES ('41e2b680-648e-4b09-bcd7-3e2b10c06264' , 'admin', 'admin', '$2b$10$yXOS0ZJ6MkTBoSqBPUW1YOEJUQdVQTHjET.tmgPf7oGfwupTrkCnO');

-- kombu/umami/prisma/migrations/02_report_schema_session_data/migration.sql
-- AlterTable
ALTER TABLE "event_data" RENAME COLUMN "event_data_type" TO "data_type";
ALTER TABLE "event_data" RENAME COLUMN "event_date_value" TO "date_value";
ALTER TABLE "event_data" RENAME COLUMN "event_id" TO "event_data_id";
ALTER TABLE "event_data" RENAME COLUMN "event_numeric_value" TO "number_value";
ALTER TABLE "event_data" RENAME COLUMN "event_string_value" TO "string_value";

-- CreateTable
CREATE TABLE "session_data" (
    "session_data_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "session_id" UUID NOT NULL,
    "session_key" VARCHAR(500) NOT NULL,
    "string_value" VARCHAR(500),
    "number_value" DECIMAL(19,4),
    "date_value" TIMESTAMPTZ(6),
    "data_type" INTEGER NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "deleted_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "session_data_pkey" PRIMARY KEY ("session_data_id")
);

-- CreateTable
CREATE TABLE "report" (
    "report_id" UUID NOT NULL,
    "user_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "type" VARCHAR(200) NOT NULL,
    "name" VARCHAR(200) NOT NULL,
    "description" VARCHAR(500) NOT NULL,
    "parameters" VARCHAR(6000) NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),

    CONSTRAINT "report_pkey" PRIMARY KEY ("report_id")
);

-- CreateIndex
CREATE INDEX "session_data_created_at_idx" ON "session_data"("created_at");

-- CreateIndex
CREATE INDEX "session_data_website_id_idx" ON "session_data"("website_id");

-- CreateIndex
CREATE INDEX "session_data_session_id_idx" ON "session_data"("session_id");

-- CreateIndex
CREATE UNIQUE INDEX "report_report_id_key" ON "report"("report_id");

-- CreateIndex
CREATE INDEX "report_user_id_idx" ON "report"("user_id");

-- CreateIndex
CREATE INDEX "report_website_id_idx" ON "report"("website_id");

-- CreateIndex
CREATE INDEX "report_type_idx" ON "report"("type");

-- CreateIndex
CREATE INDEX "report_name_idx" ON "report"("name");

-- EventData migration
UPDATE "event_data"
SET string_value = number_value
WHERE data_type = 2;

UPDATE "event_data"
SET string_value = CONCAT(REPLACE(TO_CHAR(date_value, 'YYYY-MM-DD HH24:MI:SS'), ' ', 'T'), 'Z')
WHERE data_type = 4;

-- kombu/umami/prisma/migrations/03_metric_performance_index/migration.sql
-- CreateIndex
CREATE INDEX "event_data_website_id_created_at_idx" ON "event_data"("website_id", "created_at");

-- CreateIndex
CREATE INDEX "event_data_website_id_created_at_event_key_idx" ON "event_data"("website_id", "created_at", "event_key");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_idx" ON "session"("website_id", "created_at");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_hostname_idx" ON "session"("website_id", "created_at", "hostname");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_browser_idx" ON "session"("website_id", "created_at", "browser");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_os_idx" ON "session"("website_id", "created_at", "os");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_device_idx" ON "session"("website_id", "created_at", "device");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_screen_idx" ON "session"("website_id", "created_at", "screen");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_language_idx" ON "session"("website_id", "created_at", "language");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_country_idx" ON "session"("website_id", "created_at", "country");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_subdivision1_idx" ON "session"("website_id", "created_at", "subdivision1");

-- CreateIndex
CREATE INDEX "session_website_id_created_at_city_idx" ON "session"("website_id", "created_at", "city");

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_url_path_idx" ON "website_event"("website_id", "created_at", "url_path");

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_url_query_idx" ON "website_event"("website_id", "created_at", "url_query");

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_referrer_domain_idx" ON "website_event"("website_id", "created_at", "referrer_domain");

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_page_title_idx" ON "website_event"("website_id", "created_at", "page_title");

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_event_name_idx" ON "website_event"("website_id", "created_at", "event_name");
-- kombu/umami/prisma/migrations/04_team_redesign/migration.sql
/*
  Warnings:

  - You are about to drop the `team_website` table. If the table is not empty, all the data it contains will be lost.

*/
-- AlterTable
ALTER TABLE "team" ADD COLUMN     "deleted_at" TIMESTAMPTZ(6),
ADD COLUMN     "logo_url" VARCHAR(2183);

-- AlterTable
ALTER TABLE "user" ADD COLUMN     "display_name" VARCHAR(255),
ADD COLUMN     "logo_url" VARCHAR(2183);

-- AlterTable
ALTER TABLE "website" ADD COLUMN     "created_by" UUID,
ADD COLUMN     "team_id" UUID;

-- MigrateData
UPDATE "website" SET created_by = user_id WHERE team_id IS NULL;

-- DropTable
DROP TABLE "team_website";

-- CreateIndex
CREATE INDEX "website_team_id_idx" ON "website"("team_id");

-- CreateIndex
CREATE INDEX "website_created_by_idx" ON "website"("created_by");

-- kombu/umami/prisma/migrations/05_add_visit_id/migration.sql
-- AlterTable
ALTER TABLE "website_event" ADD COLUMN "visit_id" UUID NULL;

UPDATE "website_event" we
SET visit_id = a.uuid
FROM (SELECT DISTINCT
        s.session_id,
        s.visit_time,
        gen_random_uuid() uuid
    FROM (SELECT DISTINCT session_id,
            date_trunc('hour', created_at) visit_time
        FROM "website_event") s) a
WHERE we.session_id = a.session_id 
    and date_trunc('hour', we.created_at) = a.visit_time;

ALTER TABLE "website_event" ALTER COLUMN "visit_id" SET NOT NULL;

-- CreateIndex
CREATE INDEX "website_event_visit_id_idx" ON "website_event"("visit_id");

-- CreateIndex
CREATE INDEX "website_event_website_id_visit_id_created_at_idx" ON "website_event"("website_id", "visit_id", "created_at");

-- kombu/umami/prisma/migrations/06_session_data/migration.sql
-- DropIndex
DROP INDEX IF EXISTS "event_data_website_id_created_at_event_key_idx";

-- AlterTable
ALTER TABLE "event_data" RENAME COLUMN "event_key" TO "data_key";

-- AlterTable
ALTER TABLE "session_data" DROP COLUMN "deleted_at";
ALTER TABLE "session_data" RENAME COLUMN "session_key" TO "data_key";

-- CreateIndex
CREATE INDEX "event_data_website_id_created_at_data_key_idx" ON "event_data"("website_id", "created_at", "data_key");

-- CreateIndex
CREATE INDEX "session_data_session_id_created_at_idx" ON "session_data"("session_id", "created_at");

-- CreateIndex
CREATE INDEX "session_data_website_id_created_at_data_key_idx" ON "session_data"("website_id", "created_at", "data_key");
-- kombu/umami/prisma/migrations/07_add_tag/migration.sql
-- AlterTable
ALTER TABLE "website_event" ADD COLUMN     "tag" VARCHAR(50);

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_tag_idx" ON "website_event"("website_id", "created_at", "tag");
-- kombu/umami/prisma/migrations/08_add_utm_clid/migration.sql
-- AlterTable
ALTER TABLE "website_event" 
ADD COLUMN     "fbclid" VARCHAR(255),
ADD COLUMN     "gclid" VARCHAR(255),
ADD COLUMN     "li_fat_id" VARCHAR(255),
ADD COLUMN     "msclkid" VARCHAR(255),
ADD COLUMN     "ttclid" VARCHAR(255),
ADD COLUMN     "twclid" VARCHAR(255),
ADD COLUMN     "utm_campaign" VARCHAR(255),
ADD COLUMN     "utm_content" VARCHAR(255),
ADD COLUMN     "utm_medium" VARCHAR(255),
ADD COLUMN     "utm_source" VARCHAR(255),
ADD COLUMN     "utm_term" VARCHAR(255);
-- kombu/umami/prisma/migrations/09_update_hostname_region/migration.sql
-- AlterTable
ALTER TABLE "website_event" ADD COLUMN     "hostname" VARCHAR(100);

-- DataMigration
UPDATE "website_event" w
SET hostname = s.hostname
FROM "session" s
WHERE s.website_id = w.website_id
    and s.session_id = w.session_id;

-- DropIndex
DROP INDEX IF EXISTS "session_website_id_created_at_hostname_idx";
DROP INDEX IF EXISTS "session_website_id_created_at_subdivision1_idx";

-- AlterTable
ALTER TABLE "session" RENAME COLUMN "subdivision1" TO "region";
ALTER TABLE "session" DROP COLUMN "subdivision2";
ALTER TABLE "session" DROP COLUMN "hostname";

-- CreateIndex
CREATE INDEX "website_event_website_id_created_at_hostname_idx" ON "website_event"("website_id", "created_at", "hostname");
CREATE INDEX "session_website_id_created_at_region_idx" ON "session"("website_id", "created_at", "region");



-- kombu/umami/prisma/migrations/10_add_distinct_id/migration.sql
-- AlterTable
ALTER TABLE "session" ADD COLUMN     "distinct_id" VARCHAR(50);

-- AlterTable
ALTER TABLE "session_data" ADD COLUMN     "distinct_id" VARCHAR(50);
-- kombu/umami/prisma/migrations/11_add_segment/migration.sql
-- CreateTable
CREATE TABLE "segment" (
    "segment_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "type" VARCHAR(200) NOT NULL,
    "name" VARCHAR(200) NOT NULL,
    "parameters" JSONB NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),

    CONSTRAINT "segment_pkey" PRIMARY KEY ("segment_id")
);

-- CreateIndex
CREATE UNIQUE INDEX "segment_segment_id_key" ON "segment"("segment_id");

-- CreateIndex
CREATE INDEX "segment_website_id_idx" ON "segment"("website_id");
-- kombu/umami/prisma/migrations/12_update_report_parameter/migration.sql
-- AlterTable
ALTER TABLE "report"
ALTER COLUMN "parameters" SET DATA TYPE JSONB USING parameters::JSONB;
-- kombu/umami/prisma/migrations/13_add_revenue/migration.sql
-- CreateTable
CREATE TABLE "revenue" (
    "revenue_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "session_id" UUID NOT NULL,
    "event_id" UUID NOT NULL,
    "event_name" VARCHAR(50) NOT NULL,
    "currency" VARCHAR(100) NOT NULL,
    "revenue" DECIMAL(19,4),
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "revenue_pkey" PRIMARY KEY ("revenue_id")
);

-- CreateIndex
CREATE UNIQUE INDEX "revenue_revenue_id_key" ON "revenue"("revenue_id");

-- CreateIndex
CREATE INDEX "revenue_website_id_idx" ON "revenue"("website_id");

-- CreateIndex
CREATE INDEX "revenue_session_id_idx" ON "revenue"("session_id");

-- CreateIndex
CREATE INDEX "revenue_website_id_created_at_idx" ON "revenue"("website_id", "created_at");

-- CreateIndex
CREATE INDEX "revenue_website_id_session_id_created_at_idx" ON "revenue"("website_id", "session_id", "created_at");
-- kombu/umami/prisma/migrations/14_add_link_and_pixel/migration.sql
-- AlterTable
ALTER TABLE "report" ALTER COLUMN "type" SET DATA TYPE VARCHAR(50);

-- AlterTable
ALTER TABLE "revenue" ALTER COLUMN "currency" SET DATA TYPE VARCHAR(10);

-- AlterTable
ALTER TABLE "segment" ALTER COLUMN "type" SET DATA TYPE VARCHAR(50);

-- CreateTable
CREATE TABLE "link" (
    "link_id" UUID NOT NULL,
    "name" VARCHAR(100) NOT NULL,
    "url" VARCHAR(500) NOT NULL,
    "slug" VARCHAR(100) NOT NULL,
    "user_id" UUID,
    "team_id" UUID,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),
    "deleted_at" TIMESTAMPTZ(6),

    CONSTRAINT "link_pkey" PRIMARY KEY ("link_id")
);

-- CreateTable
CREATE TABLE "pixel" (
    "pixel_id" UUID NOT NULL,
    "name" VARCHAR(100) NOT NULL,
    "slug" VARCHAR(100) NOT NULL,
    "user_id" UUID,
    "team_id" UUID,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),
    "deleted_at" TIMESTAMPTZ(6),

    CONSTRAINT "pixel_pkey" PRIMARY KEY ("pixel_id")
);

-- CreateIndex
CREATE UNIQUE INDEX "link_link_id_key" ON "link"("link_id");

-- CreateIndex
CREATE UNIQUE INDEX "link_slug_key" ON "link"("slug");

-- CreateIndex
CREATE INDEX "link_slug_idx" ON "link"("slug");

-- CreateIndex
CREATE INDEX "link_user_id_idx" ON "link"("user_id");

-- CreateIndex
CREATE INDEX "link_team_id_idx" ON "link"("team_id");

-- CreateIndex
CREATE INDEX "link_created_at_idx" ON "link"("created_at");

-- CreateIndex
CREATE UNIQUE INDEX "pixel_pixel_id_key" ON "pixel"("pixel_id");

-- CreateIndex
CREATE UNIQUE INDEX "pixel_slug_key" ON "pixel"("slug");

-- CreateIndex
CREATE INDEX "pixel_slug_idx" ON "pixel"("slug");

-- CreateIndex
CREATE INDEX "pixel_user_id_idx" ON "pixel"("user_id");

-- CreateIndex
CREATE INDEX "pixel_team_id_idx" ON "pixel"("team_id");

-- CreateIndex
CREATE INDEX "pixel_created_at_idx" ON "pixel"("created_at");

-- DataMigration Funnel
DELETE FROM "report" WHERE type = 'funnel' and jsonb_array_length(parameters->'steps') = 1;
UPDATE "report" SET parameters = parameters - 'websiteId' - 'dateRange' - 'urls' WHERE type = 'funnel';

UPDATE "report"
SET parameters = jsonb_set(
    parameters,
    '{steps}',
    (
      SELECT jsonb_agg(
               CASE
                 WHEN step->>'type' = 'url'
                 THEN jsonb_set(step, '{type}', '"path"')
                 ELSE step
               END
             )
      FROM jsonb_array_elements(parameters->'steps') step
    )
)
WHERE type = 'funnel'
    and parameters @> '{"steps":[{"type":"url"}]}';

-- DataMigration Goals
UPDATE "report" SET type = 'goal' WHERE type = 'goals';

INSERT INTO "report" (report_id, user_id, website_id, type, name, description, parameters, created_at, updated_at)
SELECT gen_random_uuid(),
    user_id,
    website_id,
    'goal',
    concat(name, ' - ', elem ->> 'value'),
    description,
    jsonb_build_object(
           'type', CASE WHEN elem ->> 'type' = 'url' THEN 'path'
                        ELSE elem ->> 'type' END,
           'value', elem ->> 'value'
       ) AS parameters,
    created_at,
    updated_at
FROM "report"
CROSS JOIN LATERAL jsonb_array_elements(parameters -> 'goals') elem
WHERE type = 'goal'
    and elem ->> 'type' IN ('event', 'url');

DELETE FROM "report" WHERE type = 'goal' and parameters ? 'goals';

-- kombu/umami/prisma/migrations/15_add_share/migration.sql
-- CreateTable
CREATE TABLE "share" (
    "share_id" UUID NOT NULL,
    "entity_id" UUID NOT NULL,
    "name" VARCHAR(200) NOT NULL,
    "share_type" INTEGER NOT NULL,
    "slug" VARCHAR(100) NOT NULL,
    "parameters" JSONB NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),

    CONSTRAINT "share_pkey" PRIMARY KEY ("share_id")
);

-- CreateIndex
CREATE UNIQUE INDEX "share_slug_key" ON "share"("slug");

-- CreateIndex
CREATE INDEX "share_entity_id_idx" ON "share"("entity_id");

-- MigrateData
INSERT INTO "share" (share_id, entity_id, name, share_type, slug, parameters, created_at)
SELECT gen_random_uuid(),
       website_id,
       name,
       1,
       share_id,
       '{"overview":true}'::jsonb,
       now()
FROM "website"
WHERE share_id IS NOT NULL;

-- DropIndex
DROP INDEX "website_share_id_idx";

-- DropIndex
DROP INDEX "website_share_id_key";

-- AlterTable
ALTER TABLE "website" DROP COLUMN "share_id";
-- kombu/umami/prisma/migrations/16_boards/migration.sql
-- CreateTable
CREATE TABLE "board" (
    "board_id" UUID NOT NULL,
    "type" VARCHAR(50) NOT NULL,
    "name" VARCHAR(200) NOT NULL,
    "description" VARCHAR(500) NOT NULL,
    "parameters" JSONB NOT NULL,
    "slug" VARCHAR(100) NOT NULL,
    "user_id" UUID,
    "team_id" UUID,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),

    CONSTRAINT "board_pkey" PRIMARY KEY ("board_id")
);

-- CreateIndex
CREATE UNIQUE INDEX "board_slug_key" ON "board"("slug");

-- CreateIndex
CREATE INDEX "board_slug_idx" ON "board"("slug");

-- CreateIndex
CREATE INDEX "board_user_id_idx" ON "board"("user_id");

-- CreateIndex
CREATE INDEX "board_team_id_idx" ON "board"("team_id");

-- CreateIndex
CREATE INDEX "board_created_at_idx" ON "board"("created_at");
-- kombu/umami/prisma/migrations/17_remove_duplicate_key/migration.sql
-- DropIndex
DROP INDEX "link_link_id_key";

-- DropIndex
DROP INDEX "pixel_pixel_id_key";

-- DropIndex
DROP INDEX "report_report_id_key";

-- DropIndex
DROP INDEX "revenue_revenue_id_key";

-- DropIndex
DROP INDEX "segment_segment_id_key";

-- DropIndex
DROP INDEX "session_session_id_key";

-- DropIndex
DROP INDEX "team_team_id_key";

-- DropIndex
DROP INDEX "team_user_team_user_id_key";

-- DropIndex
DROP INDEX "user_user_id_key";

-- DropIndex
DROP INDEX "website_website_id_key";
-- kombu/umami/prisma/migrations/18_add_performance/migration.sql
-- AlterTable
ALTER TABLE "website_event" ADD COLUMN     "cls" DECIMAL(10,4),
ADD COLUMN     "fcp" DECIMAL(10,1),
ADD COLUMN     "inp" DECIMAL(10,1),
ADD COLUMN     "lcp" DECIMAL(10,1),
ADD COLUMN     "ttfb" DECIMAL(10,1);
-- kombu/umami/prisma/migrations/19_add_session_replay/migration.sql
-- AlterTable board: drop slug
DROP INDEX IF EXISTS "board_slug_key";
DROP INDEX IF EXISTS "board_slug_idx";
ALTER TABLE "board" DROP COLUMN IF EXISTS "slug";

-- AlterTable
ALTER TABLE "website" ADD COLUMN "replay_enabled" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "website" ADD COLUMN "replay_config" JSONB;

-- CreateTable
CREATE TABLE "session_replay" (
    "replay_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "session_id" UUID NOT NULL,
    "visit_id" UUID NOT NULL,
    "chunk_index" INTEGER NOT NULL,
    "events" BYTEA NOT NULL,
    "event_count" INTEGER NOT NULL,
    "started_at" TIMESTAMPTZ(6) NOT NULL,
    "ended_at" TIMESTAMPTZ(6) NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "session_replay_pkey" PRIMARY KEY ("replay_id")
);

-- CreateIndex
CREATE INDEX "session_replay_website_id_idx" ON "session_replay"("website_id");
CREATE INDEX "session_replay_session_id_idx" ON "session_replay"("session_id");
CREATE INDEX "session_replay_website_id_session_id_idx" ON "session_replay"("website_id", "session_id");
CREATE INDEX "session_replay_website_id_visit_id_idx" ON "session_replay"("website_id", "visit_id");
CREATE INDEX "session_replay_website_id_created_at_idx" ON "session_replay"("website_id", "created_at");
CREATE INDEX "session_replay_session_id_chunk_index_idx" ON "session_replay"("session_id", "chunk_index");

-- CreateTable
CREATE TABLE "session_replay_saved" (
    "saved_replay_id" UUID NOT NULL,
    "name" VARCHAR(100) NOT NULL,
    "website_id" UUID NOT NULL,
    "visit_id" UUID NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6),

    CONSTRAINT "session_replay_saved_pkey" PRIMARY KEY ("saved_replay_id"),
    CONSTRAINT "session_replay_saved_website_id_visit_id_key" UNIQUE ("website_id", "visit_id")
);

-- CreateIndex
CREATE INDEX "session_replay_saved_website_id_idx" ON "session_replay_saved"("website_id");
CREATE INDEX "session_replay_saved_visit_id_idx" ON "session_replay_saved"("visit_id");
CREATE INDEX "session_replay_saved_website_id_created_at_idx" ON "session_replay_saved"("website_id", "created_at");
-- kombu/umami/prisma/migrations/20_add_heatmap/migration.sql
ALTER TABLE "website" RENAME COLUMN "replay_enabled" TO "recorder_enabled";

UPDATE "website"
SET "replay_config" = COALESCE("replay_config", '{}'::jsonb) || '{"replayEnabled": true}'::jsonb
WHERE "recorder_enabled" = true;

-- CreateTable
CREATE TABLE "heatmap_event" (
    "heatmap_event_id" UUID NOT NULL,
    "website_id" UUID NOT NULL,
    "session_id" UUID NOT NULL,
    "visit_id" UUID NOT NULL,
    "url_path" VARCHAR(500) NOT NULL,
    "event_type" INTEGER NOT NULL,
    "x" INTEGER,
    "y" INTEGER,
    "page_x" INTEGER,
    "page_y" INTEGER,
    "page_w" INTEGER,
    "viewport_w" INTEGER,
    "viewport_h" INTEGER,
    "page_h" INTEGER,
    "scroll_pct" INTEGER,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "heatmap_event_pkey" PRIMARY KEY ("heatmap_event_id")
);

-- CreateIndex
CREATE INDEX "heatmap_event_website_id_idx" ON "heatmap_event"("website_id");
CREATE INDEX "heatmap_event_visit_id_idx" ON "heatmap_event"("visit_id");
CREATE INDEX "heatmap_event_website_id_created_at_idx" ON "heatmap_event"("website_id", "created_at");
CREATE INDEX "heatmap_event_website_id_url_path_event_type_created_at_idx" ON "heatmap_event"("website_id", "url_path", "event_type", "created_at");

-- kombu/umami/prisma/migrations/21_add_session_link/migration.sql
-- CreateTable
CREATE TABLE "session_link" (
    "website_id" UUID NOT NULL,
    "session_id" UUID NOT NULL,
    "distinct_id" VARCHAR(50) NOT NULL,
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "session_link_pkey" PRIMARY KEY ("website_id","distinct_id","session_id")
);

CREATE INDEX "session_link_website_id_session_id_idx" ON "session_link"("website_id", "session_id");
-- kombu/umami/prisma/migrations/22_add_2fa/migration.sql
-- AlterTable
ALTER TABLE "team" ADD COLUMN     "two_factor_required" BOOLEAN NOT NULL DEFAULT false;

-- AlterTable
ALTER TABLE "user" ADD COLUMN     "two_factor_required" BOOLEAN NOT NULL DEFAULT false;

-- CreateTable
CREATE TABLE "two_factor_auth" (
    "id" TEXT NOT NULL,
    "user_id" UUID NOT NULL,
    "secret" TEXT NOT NULL,
    "is_enabled" BOOLEAN NOT NULL DEFAULT false,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL,

    CONSTRAINT "two_factor_auth_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "two_factor_backup_code" (
    "id" TEXT NOT NULL,
    "user_id" UUID NOT NULL,
    "code_hash" TEXT NOT NULL,
    "used" BOOLEAN NOT NULL DEFAULT false,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "two_factor_backup_code_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "two_factor_otp_used" (
    "id" TEXT NOT NULL,
    "user_id" UUID NOT NULL,
    "otp" TEXT NOT NULL,
    "expires_at" TIMESTAMPTZ(6) NOT NULL,

    CONSTRAINT "two_factor_otp_used_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "two_factor_rate_limit" (
    "id" TEXT NOT NULL,
    "user_id" UUID NOT NULL,
    "attempts" INTEGER NOT NULL DEFAULT 0,
    "locked_until" TIMESTAMPTZ(6),
    "updated_at" TIMESTAMPTZ(6) NOT NULL,

    CONSTRAINT "two_factor_rate_limit_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "app_setting" (
    "key" TEXT NOT NULL,
    "value" TEXT NOT NULL,

    CONSTRAINT "app_setting_pkey" PRIMARY KEY ("key")
);

-- CreateIndex
CREATE UNIQUE INDEX "two_factor_auth_user_id_key" ON "two_factor_auth"("user_id");

-- CreateIndex
CREATE INDEX "two_factor_backup_code_user_id_idx" ON "two_factor_backup_code"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "two_factor_otp_used_user_id_otp_key" ON "two_factor_otp_used"("user_id", "otp");

-- CreateIndex
CREATE UNIQUE INDEX "two_factor_rate_limit_user_id_key" ON "two_factor_rate_limit"("user_id");

-- Historical catch-up: some environments already have this replay index,
-- but the current schema expects it everywhere.
CREATE INDEX IF NOT EXISTS "session_replay_visit_id_idx" ON "session_replay"("visit_id");

-- kombu/umami/prisma/migrations/23_update_session_data/migration.sql
WITH ranked_session_data AS (
  SELECT
    "session_data_id",
    ROW_NUMBER() OVER (
      PARTITION BY "session_id", "data_key"
      ORDER BY "created_at" DESC NULLS LAST, "session_data_id" DESC
    ) AS row_num
  FROM "session_data"
)
DELETE FROM "session_data"
USING ranked_session_data
WHERE "session_data"."session_data_id" = ranked_session_data."session_data_id"
  AND ranked_session_data.row_num > 1;

CREATE UNIQUE INDEX "session_data_session_id_data_key_key"
ON "session_data"("session_id", "data_key");
-- add final schema fix: ensure app_setting exists
CREATE TABLE IF NOT EXISTS "app_setting" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL);
