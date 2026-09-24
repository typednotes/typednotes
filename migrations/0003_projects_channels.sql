-- Projects, more connection kinds, and messaging channels
-- (docs/connections.md §3, §10, §11).
--
-- - Connections gain GitLab, Dropbox and Azure Blob Storage, plus the three
--   messaging providers (Slack, WhatsApp, Signal). `external_id` records the
--   provider-side identity a connection stands for, where the app needs it
--   without the credential: a Slack team id, a WhatsApp phone number id, a
--   Signal number.
-- - OAuth round trips can start from a project page and come back to it
--   (`return_to`), and run against GitLab, Dropbox and Slack.
-- - A project belongs to an org and may name one primary repository, read
--   through a GitHub or GitLab connection.
-- - A channel binds one inbound address (a Slack channel, a WhatsApp number,
--   a Signal number) to one project; its messages, both ways, are the
--   project's inbox. Still no credential in any of these tables.

-- ── Connections ─────────────────────────────────────────────────────────

alter table connections drop constraint connections_provider_check;
alter table connections add constraint connections_provider_check check (provider in
    ('github', 'gitlab', 'gdrive', 'dropbox', 's3', 'azure',
     'mistral', 'openai', 'anthropic', 'openai-compatible',
     'slack', 'whatsapp', 'signal'));

alter table connections add column external_id text;

-- ── OAuth round trips ───────────────────────────────────────────────────

alter table oauth_flows drop constraint oauth_flows_idp_check;
alter table oauth_flows add constraint oauth_flows_idp_check check (idp in
    ('github', 'google', 'gitlab', 'dropbox', 'slack'));

alter table oauth_flows drop constraint oauth_flows_connection_provider_check;
alter table oauth_flows add constraint oauth_flows_connection_provider_check
    check (connection_provider in ('github', 'gitlab', 'gdrive', 'dropbox', 'slack'));

-- A same-origin path (`/orgs/{slug}/projects/{project}`) to come back to
-- instead of the org page; `connect` flows only.
alter table oauth_flows add column return_to text
    check (return_to is null or (purpose = 'connect' and return_to like '/orgs/%'));

-- ── Projects ────────────────────────────────────────────────────────────

create table projects (
    id                  uuid primary key default gen_random_uuid(),
    org_id              uuid not null references orgs(id) on delete cascade,
    slug                citext not null,
    name                text not null,
    created_by          uuid references users(id) on delete set null,
    created_at          timestamptz not null default now(),
    -- The primary repository, as last read through `repo_connection_id`.
    -- Removing that connection keeps the repository's name, not the access.
    repo_connection_id  uuid references connections(id) on delete set null,
    repo_provider       text check (repo_provider in ('github', 'gitlab')),
    repo_full_name      text,                -- owner/name, or group/sub/name
    repo_web_url        text,
    repo_default_branch text,
    unique (org_id, slug),
    check ((repo_provider is null) = (repo_full_name is null)
       and (repo_provider is null) = (repo_web_url is null))
);

-- ── Channels and their messages ─────────────────────────────────────────

create table channels (
    id               uuid primary key default gen_random_uuid(),
    project_id       uuid not null references projects(id) on delete cascade,
    connection_id    uuid not null references connections(id) on delete cascade,
    provider         text not null check (provider in ('slack', 'whatsapp', 'signal')),
    -- Where inbound messages are addressed: the Slack team id, the WhatsApp
    -- phone number id, the Signal number.
    external_id      text not null,
    -- The Slack channel id; null for WhatsApp and Signal, where the number
    -- is the channel.
    external_channel text,
    label            text not null,
    created_at       timestamptz not null default now(),
    -- An inbound address routes to exactly one project.
    unique nulls not distinct (provider, external_id, external_channel)
);

create index on channels (project_id);

create table channel_messages (
    id          uuid primary key default gen_random_uuid(),
    channel_id  uuid not null references channels(id) on delete cascade,
    direction   text not null check (direction in ('in', 'out')),
    -- The provider's id for the message (Slack `ts`, WhatsApp `wamid`, Signal
    -- timestamp): webhook retries and repeated pulls insert nothing new.
    external_id text,
    -- The sender of an inbound message, the recipient of an outbound one: a
    -- Slack user or channel id, a phone number.
    peer        text not null,
    peer_name   text,
    body        text not null,
    sent_by     uuid references users(id) on delete set null,
    created_at  timestamptz not null default now(),
    unique (channel_id, direction, external_id)
);

create index on channel_messages (channel_id, created_at desc);
