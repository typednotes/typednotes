create table notebook_shares (
    id uuid primary key default gen_random_uuid(),
    graph_id uuid not null references graphs(id) on delete cascade,
    owner_id uuid not null references users(id) on delete cascade,
    token_hash bytea not null unique,
    build_id text not null,
    snapshot jsonb not null,
    initial_inputs jsonb not null,
    created_at timestamptz not null default now()
);
create table notebook_share_sessions (
    id_hash bytea primary key,
    share_id uuid not null references notebook_shares(id) on delete cascade,
    lun_session_id text,
    inputs jsonb not null,
    nodes jsonb not null,
    expires_at timestamptz not null
);
create table notebook_share_calls (
    share_id uuid not null references notebook_shares(id) on delete cascade,
    at timestamptz not null default now()
);
create index notebook_share_calls_recent on notebook_share_calls(share_id, at);
