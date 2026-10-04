-- Typednotes owns durable occurrences; Lun returns bounded graph steps and
-- graph-internal deadlines. No SQL migration starts work or repeats effects.
alter table graphs add column runtime_revision bigint not null default 0 check(runtime_revision>=0);
create table scheduled_runs (
  id uuid primary key default gen_random_uuid(),
  occurrence_key text not null unique,
  graph_id uuid not null references graphs(id) on delete cascade,
  cell_id uuid references graph_cells(id) on delete cascade,
  actor_id uuid references users(id) on delete set null,
  kind text not null check(kind in ('source','producer')),
  due_at timestamptz not null,
  build_id text not null,
  runtime_revision bigint,
  declaration jsonb,
  input_value jsonb,
  state text not null default 'queued' check(state in ('queued','running','input_ready','completed','failed','uncertain','cancelled')),
  phase text not null default 'source' check(phase in ('source','graph')),
  detail text,
  started_at timestamptz,
  phase_started_at timestamptz,
  finished_at timestamptz,
  created_at timestamptz not null default now(),
  check((kind='source')=(cell_id is not null)),
  check(kind<>'producer' or (runtime_revision is not null and phase='graph'))
);
create index scheduled_runs_pending on scheduled_runs(created_at) where state in ('queued','input_ready');
create index scheduled_runs_history on scheduled_runs(graph_id,created_at desc);
alter table graph_updates add column scheduled_run_id uuid references scheduled_runs(id) on delete set null;
alter table graph_updates add column runtime_revision bigint;
alter table source_checks add column scheduled_run_id uuid references scheduled_runs(id) on delete set null;
-- Ordered application-side channel emissions are an outbox, not a replay of a
-- graph step. Each row is sent with fresh project/connection/actor authority.
create table graph_delivery_outbox (
  id uuid primary key default gen_random_uuid(),
  graph_id uuid not null references graphs(id) on delete cascade,
  update_id uuid not null references graph_updates(id) on delete cascade,
  emission_index integer not null check(emission_index>=0),
  runtime_revision bigint not null,
  cell_id uuid not null references graph_cells(id) on delete cascade,
  actor_id uuid references users(id) on delete set null,
  channel_id uuid not null references channels(id) on delete cascade,
  recipient text not null,
  body text not null,
  state text not null default 'pending' check(state in ('pending','sending','sent','failed','uncertain','cancelled')),
  detail text,
  started_at timestamptz,
  finished_at timestamptz,
  created_at timestamptz not null default now(),
  unique(update_id,emission_index,cell_id)
);
create index graph_delivery_pending on graph_delivery_outbox(graph_id,created_at,emission_index)
  where state in ('pending','sending','uncertain');
-- Supported cadence policy: coalesce overdue source checks; serialize graph
-- steps. An uncertain dispatch blocks automatic work until an explicit restart.
create unique index scheduled_source_active on scheduled_runs(cell_id)
  where kind='source' and state in ('queued','running','input_ready','uncertain');
create unique index scheduled_producer_active on scheduled_runs(graph_id)
  where kind='producer' and state in ('queued','running','uncertain');
