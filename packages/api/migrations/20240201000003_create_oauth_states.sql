-- Create oauth_states table for CSRF protection during OAuth flow
CREATE TABLE IF NOT EXISTS oauth_states (
    state VARCHAR(255) PRIMARY KEY,
    provider VARCHAR(50) NOT NULL,
    pkce_verifier TEXT NOT NULL,
    redirect_uri TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

-- Index for cleanup of expired states
CREATE INDEX IF NOT EXISTS idx_oauth_states_expires ON oauth_states(expires_at);
