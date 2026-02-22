-- Add password hash column for local authentication
ALTER TABLE users ADD COLUMN password_hash TEXT;

-- Create git configuration table (1:1 with users)
CREATE TABLE IF NOT EXISTS user_git_config (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    git_remote_url TEXT,
    ssh_private_key_enc BYTEA,
    ssh_public_key TEXT,
    encryption_nonce BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
