-- Migration 03: Custom pixel images
ALTER TABLE "pixel" ADD COLUMN IF NOT EXISTS "image_data" BYTEA;
ALTER TABLE "pixel" ADD COLUMN IF NOT EXISTS "content_type" VARCHAR(50);
