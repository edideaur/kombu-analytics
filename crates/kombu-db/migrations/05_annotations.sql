-- Migration 05: Website Annotations and Release Markers
CREATE TABLE IF NOT EXISTS "website_annotation" (
    "annotation_id" UUID NOT NULL PRIMARY KEY,
    "website_id" UUID NOT NULL,
    "user_id" UUID,
    "date" TIMESTAMPTZ(6) NOT NULL,
    "title" VARCHAR(255) NOT NULL,
    "description" TEXT,
    "color" VARCHAR(50) NOT NULL DEFAULT 'primary',
    "created_at" TIMESTAMPTZ(6) DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6)
);

CREATE INDEX IF NOT EXISTS "website_annotation_website_id_idx" ON "website_annotation"("website_id");
CREATE INDEX IF NOT EXISTS "website_annotation_date_idx" ON "website_annotation"("date");
