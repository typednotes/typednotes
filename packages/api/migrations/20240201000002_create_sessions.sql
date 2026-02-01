-- Create sessions table for tower-sessions-sqlx-store
-- This schema is required by the PostgresStore
CREATE TABLE IF NOT EXISTS tower_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    data BYTEA NOT NULL,
    expiry_date TIMESTAMPTZ NOT NULL
);

-- Index for faster expiry cleanup
CREATE INDEX IF NOT EXISTS idx_tower_sessions_expiry ON tower_sessions(expiry_date);
