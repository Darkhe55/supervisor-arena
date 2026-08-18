-- Local dev database initialization
-- Runs once on first `docker compose up`

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
-- For full-text search (H-4)
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Note: schema migrations are managed by sqlx migrate
-- This file is for extensions + initial setup only
