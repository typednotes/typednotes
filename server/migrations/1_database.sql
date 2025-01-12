-- Create users table
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    is_active BOOLEAN,
    full_name VARCHAR(255),
    avatar_url TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Insert users
INSERT INTO users
    (username, email, is_active, full_name, avatar_url) SELECT 'admin', 'admin@typednotes.org', true, 'Admin', NULL
ON CONFLICT(id) DO UPDATE SET username = EXCLUDED.username, email = EXCLUDED.email, is_active = EXCLUDED.is_active

-- Attach permissions
CREATE TABLE IF NOT EXISTS user_permissions (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL,
    token VARCHAR(256) NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id)
);

-- Insert permissions
INSERT INTO user_permissions (user_id, token) SELECT 1, 'test'