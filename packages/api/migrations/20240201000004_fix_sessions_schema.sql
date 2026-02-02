-- Fix sessions table schema for tower-sessions-sqlx-store 0.15+
-- The new version expects tower_sessions.session instead of tower_sessions table

-- Create the schema
CREATE SCHEMA IF NOT EXISTS tower_sessions;

-- Create the new table in the correct schema
CREATE TABLE IF NOT EXISTS tower_sessions.session (
    id TEXT PRIMARY KEY NOT NULL,
    data BYTEA NOT NULL,
    expiry_date TIMESTAMPTZ NOT NULL
);

-- Index for faster expiry cleanup
CREATE INDEX IF NOT EXISTS idx_tower_sessions_session_expiry ON tower_sessions.session(expiry_date);

-- Drop the old table (in default schema)
DROP TABLE IF EXISTS public.tower_sessions;
