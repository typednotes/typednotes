-- Trusted minting projections are revocable independently of five-minute HMACs.
create table connector_authorities (
    warrant_id uuid primary key,
    org_id uuid not null references orgs(id) on delete cascade,
    -- External connections use a row UUID. Local services use synthetic named
    -- references (compute / graph UUID), owned by the authenticated actor.
    connection_id uuid references connections(id) on delete cascade,
    connection_ref text not null,
    owner_id uuid not null references users(id) on delete cascade,
    run_id uuid not null,
    account text not null,
    provider text not null,
    graph_id uuid references graphs(id) on delete cascade,
    cell_id uuid references graph_cells(id) on delete cascade,
    -- Non-secret minting projection retained so the write-only app can bind
    -- writer conversation/publication policy without reading any vault secret.
    projection jsonb not null,
    expires_at timestamptz not null
);
create index connector_authorities_org_connection on connector_authorities(org_id, connection_id);
create index connector_authorities_graph_cell on connector_authorities(graph_id, cell_id);
create index connector_authorities_named_connection on connector_authorities(org_id, provider, connection_ref);
