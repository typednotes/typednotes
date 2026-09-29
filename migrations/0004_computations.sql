-- Computations (docs/computations.md §7): notebooks whose cells are prose
-- descriptions of the nodes of a reactive graph, implemented by `lode` and
-- run by `lun`.
--
-- The cells are the only state that cannot be reconstructed: lode's work is
-- re-derived from them, lun's build from the repository, and a lun session
-- from `graph_inputs`, the log of every input the app fed. A secret's value
-- never appears in any of these tables (it is in the vault, at
-- secret/data/graph/{org_id}/{graph_id}/{name}); nor does an endpoint's
-- token (only its SHA-256), nor a compute role's password (only in the
-- vault, at secret/data/compute/{org_id}/{user_id}).

-- ── Org settings ────────────────────────────────────────────────────────

-- Rewrites the app may launch by itself (a failed build, a failing node)
-- before a member asks for one again; null is the deployment's default
-- (TYPEDNOTES_AUTO_REPAIRS). Set on the org's settings page.
alter table orgs add column auto_repairs integer
    check (auto_repairs is null or auto_repairs between 0 and 10);

-- ── Graphs and their cells ──────────────────────────────────────────────

create table graphs (
    id                  uuid primary key default gen_random_uuid(),
    project_id          uuid not null references projects(id) on delete cascade,
    slug                citext not null,
    name                text not null,
    created_by          uuid references users(id) on delete set null,
    status              text not null default 'editing'
                            check (status in ('editing', 'implementing', 'ready', 'failed')),
    -- Why it failed, or the step it is at.
    status_detail       text,
    -- The AI connection lode's model calls go through, and the model.
    model_connection_id uuid references connections(id) on delete set null,
    model_name          text,
    lode_session_id     text,
    -- Where lode's current run starts in its session log, so a cell shows
    -- the progress of this run only.
    lode_log_start      integer not null default 0,
    -- Rewrites the app launched by itself (a failed build, an error lun
    -- reported) since a member last asked for one: bounded by
    -- TYPEDNOTES_AUTO_REPAIRS, reset by the next member's request.
    auto_repairs        integer not null default 0,
    -- The published commit lode reported, and lun's build of it.
    commit_sha          text,
    lun_build_id        text,
    -- The live session and the user it is bound to: lun's `PostgreSQL` and
    -- `SecretStore` handlers resolve their capabilities from
    -- (org, session_user_id, graph).
    lun_session_id      text,
    -- The ready build the session runs: the code the cells execute, while
    -- lode may be writing (and lun building) the next one as `lun_build_id`.
    session_build_id    text,
    session_user_id     uuid references users(id) on delete set null,
    -- The nodes of the last answer, for the notebook.
    last_nodes          jsonb,
    -- lun's reading of the graph (node ids): what reads nothing, what
    -- nothing reads.
    structure           jsonb,
    created_at          timestamptz not null default now(),
    updated_at          timestamptz not null default now(),
    unique (project_id, slug)
);

create table graph_cells (
    id          uuid primary key default gen_random_uuid(),
    graph_id    uuid not null references graphs(id) on delete cascade,
    position    integer not null,
    -- The identifier lode names the cell's function (and its input) after.
    name        text not null check (name ~ '^[a-z][a-z0-9_]{0,39}$'),
    kind        text not null check (kind in ('node', 'source', 'sink')),
    -- A source's trigger, a sink's kind; none for a plain node.
    variant     text,
    description text not null,
    -- By trigger or kind (docs/computations.md §7):
    --   scheduled, watch: {url, schedule (cron), input}
    --   ui:       {input, choices?}          (source)
    --   secret:   {name, set_at?}            — never the value
    --   endpoint: {token_hash, input}        — sha256 of the token, hex
    --   channel:  {channel_id, input}        (source)
    --   db:       {table}
    --   http:     {url}
    --   storage:  {connection_id, path}
    --   ui:       {format}                   (sink)
    --   channel:  {channel_id, recipient?}   (sink)
    config      jsonb not null default '{}',
    -- Once lode succeeded: {function, module, signature, effects, input, input_type}.
    impl        jsonb,
    -- lode is (re)writing this cell's code: set when a run is launched for
    -- it, cleared when the run's build is adopted.
    writing     boolean not null default false,
    -- Why the code is being (or was last) rewritten: a member's report, an
    -- error lun reported, a failed build.
    issue       text,
    -- The last lun error a rewrite was launched for by the app itself, so
    -- the same error does not launch another.
    auto_issue  text,
    -- When the current code was adopted, and when the prose or config last
    -- changed: code older than the prose is out of date.
    code_at     timestamptz,
    edited_at   timestamptz not null default now(),
    -- `scheduled` and `watch`: the due table the tick reads.
    next_due_at timestamptz,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    unique (graph_id, name),
    check ((kind = 'node') = (variant is null)),
    check (kind <> 'source'
        or variant in ('scheduled', 'watch', 'ui', 'secret', 'endpoint', 'channel')),
    check (kind <> 'sink' or variant in ('db', 'http', 'storage', 'ui', 'channel')),
    check (next_due_at is null or variant in ('scheduled', 'watch'))
);

create index on graph_cells (graph_id, position);
create index on graph_cells (next_due_at) where next_due_at is not null;
-- An endpoint is looked up by its token's hash, and a token is one cell's.
create unique index graph_cells_endpoint_token
    on graph_cells ((config ->> 'token_hash'))
    where variant = 'endpoint' and config ? 'token_hash';

-- ── What the app fed, and what changed ──────────────────────────────────

-- Every input the app ever fed a session (ui edits, scheduled checks, watch
-- changes, endpoint deliveries, channel messages): the log that re-registers
-- a lost lun session and that a `watch` source compares against.
create table graph_inputs (
    id       uuid primary key default gen_random_uuid(),
    graph_id uuid not null references graphs(id) on delete cascade,
    input    text not null,
    value    jsonb not null,
    fed_by   text not null check (fed_by in ('ui', 'check', 'endpoint', 'channel')),
    cell_id  uuid references graph_cells(id) on delete set null,
    at       timestamptz not null default now()
);

create index on graph_inputs (graph_id, input, at desc);

-- Every session update the app drove, with what changed: the notebook's
-- history and the audit trail for sinks that fired inside the recompute.
create table graph_updates (
    id       uuid primary key default gen_random_uuid(),
    graph_id uuid not null references graphs(id) on delete cascade,
    fed_by   text not null check (fed_by in ('ui', 'check', 'endpoint', 'channel', 'register')),
    cell_id  uuid references graph_cells(id) on delete set null,
    changed  jsonb not null,
    at       timestamptz not null default now()
);

create index on graph_updates (graph_id, at desc);

-- One row per scheduled or watch check (docs/computations.md §3.1–3.2).
create table source_checks (
    id         uuid primary key default gen_random_uuid(),
    cell_id    uuid not null references graph_cells(id) on delete cascade,
    -- The org, for the per-org hourly cap.
    org_id     uuid not null references orgs(id) on delete cascade,
    started_at timestamptz not null default now(),
    latency_ms integer,
    ok         boolean not null,
    outcome    text not null,
    -- When its value was fed (a watch that saw no change feeds nothing).
    fed_at     timestamptz
);

create index on source_checks (cell_id, started_at desc);
create index on source_checks (org_id, started_at desc);

-- One row per endpoint delivery (docs/computations.md §3.5).
create table endpoint_calls (
    id      uuid primary key default gen_random_uuid(),
    cell_id uuid not null references graph_cells(id) on delete cascade,
    at      timestamptz not null default now(),
    ok      boolean not null,
    status  integer not null,
    fed_at  timestamptz
);

create index on endpoint_calls (cell_id, at desc);

-- ── Per-user compute (docs/computations.md §4.1) ────────────────────────

-- Which (org, user) got a schema and role on compute-db, lazily, on the
-- first session whose graph has a `db` sink. The role's password is only in
-- the vault.
create table compute_schemas (
    org_id     uuid not null references orgs(id) on delete cascade,
    user_id    uuid not null references users(id) on delete cascade,
    name       text not null unique check (name ~ '^[a-z0-9_]{1,63}$'),
    created_at timestamptz not null default now(),
    primary key (org_id, user_id)
);
