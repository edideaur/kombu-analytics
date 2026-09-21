-- 07_community_features.sql
-- Add Open Graph fields to link table
ALTER TABLE "link" ADD COLUMN IF NOT EXISTS "og_title" VARCHAR(200);
ALTER TABLE "link" ADD COLUMN IF NOT EXISTS "og_description" TEXT;
ALTER TABLE "link" ADD COLUMN IF NOT EXISTS "og_image_url" VARCHAR(500);

-- Team invitation links with cryptographic token hash, expiration and revocation
CREATE TABLE IF NOT EXISTS "team_invitation" (
    "invitation_id" UUID PRIMARY KEY,
    "team_id" UUID NOT NULL REFERENCES "team"("team_id") ON DELETE CASCADE,
    "role" VARCHAR(50) NOT NULL DEFAULT 'team-member',
    "token_hash" VARCHAR(128) NOT NULL,
    "created_by" UUID NOT NULL REFERENCES "user"("user_id") ON DELETE CASCADE,
    "expires_at" TIMESTAMPTZ(6) NOT NULL,
    "accepted_at" TIMESTAMPTZ(6),
    "accepted_by" UUID REFERENCES "user"("user_id") ON DELETE SET NULL,
    "revoked_at" TIMESTAMPTZ(6),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS "team_invitation_team_id_idx" ON "team_invitation"("team_id");
CREATE INDEX IF NOT EXISTS "team_invitation_token_hash_idx" ON "team_invitation"("token_hash");

-- External SSO/OIDC auth identity mapping
CREATE TABLE IF NOT EXISTS "user_external_auth" (
    "id" UUID PRIMARY KEY,
    "user_id" UUID NOT NULL REFERENCES "user"("user_id") ON DELETE CASCADE,
    "provider" VARCHAR(50) NOT NULL,
    "subject" VARCHAR(255) NOT NULL,
    "email" VARCHAR(255),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "user_external_auth_provider_subject_key" UNIQUE ("provider", "subject")
);
CREATE INDEX IF NOT EXISTS "user_external_auth_user_id_idx" ON "user_external_auth"("user_id");
