-- Sign-in and connections (docs/connections.md): browser sessions, the
-- short-lived state of an OAuth round trip, and the third-party accounts an
-- org's members connect.
--
-- No credential is stored here. A connection's token, key or refresh token
-- lives in typednotes/secrets at
-- secret/data/thirdparty/{provider}/{user_id}/{connection_id}; this table
-- only says the connection exists, who made it, and how its last test went.
-- The app can write that vault path but not read it back.

-- A signed-in browser. The cookie carries 32 random bytes; only their SHA-256
-- is stored, so a leaked table cannot be replayed as a cookie.
create table sessions (
    id_hash    bytea primary key,
    user_id    uuid not null references users(id) on delete cascade,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);

create index on sessions (user_id);
create index on sessions (expires_at);

-- One pending authorization-code round trip: the `state` sent to the
-- provider, the PKCE verifier kept back, and what the round trip is for.
-- Consumed (deleted) by the callback, so a `state` is single-use.
create table oauth_flows (
    state               text primary key,
    idp                 text not null check (idp in ('github', 'google')),
    purpose             text not null check (purpose in ('login', 'connect')),
    pkce_verifier       text not null,
    -- `connect` only: who asked, for which org, and which connection kind.
    user_id             uuid references users(id) on delete cascade,
    org_id              uuid references orgs(id) on delete cascade,
    connection_provider text check (connection_provider in ('github', 'gdrive')),
    created_at          timestamptz not null default now(),
    expires_at          timestamptz not null,
    check ((purpose = 'login')
        = (user_id is null and org_id is null and connection_provider is null))
);

create index on oauth_flows (expires_at);

-- A third-party account connected to an org by one of its members. The
-- provider ids are the ones liaison's warrants and the vault path use.
create table connections (
    id              uuid primary key default gen_random_uuid(),
    org_id          uuid not null references orgs(id) on delete cascade,
    user_id         uuid not null references users(id) on delete cascade,
    provider        text not null check (provider in
                        ('github', 'gdrive', 's3', 'mistral', 'openai', 'anthropic',
                         'openai-compatible')),
    label           text not null,           -- GitHub login, Google email, bucket…
    base_url        text not null,           -- liaison refuses URLs outside it
    -- 'pending' until the credential is in the vault; 'failed' after a failed test.
    status          text not null default 'pending'
                        check (status in ('pending', 'active', 'failed')),
    last_checked_at timestamptz,
    last_error      text,
    created_at      timestamptz not null default now()
);

create index on connections (org_id, created_at);
