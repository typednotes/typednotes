-- Core identity schema, verbatim from docs/services/core.md §4: users, the
-- external identities that map onto them, orgs, and memberships. Every other
-- table in the system keys off users.id / orgs.id — ledger's usage_events and
-- credit_* reference orgs(id) and users(id), which is why typednotes-infra
-- orders ledger's history after this one (infra's `after` edge).
--
-- Applied in production by typednotes-infra as a declared postgresMigrations
-- history, read from this file at the release tag; the app server never
-- migrates. Locally: scripts/dev-db.sh.
--
-- `citext` is on Scaleway Serverless SQL's supported-extensions list
-- (checked 2026-09-23); it is a trusted extension, so the DDL identity can
-- create it. gen_random_uuid() is built in since PostgreSQL 13.

create extension if not exists citext;

create table users (
    id           uuid primary key default gen_random_uuid(),
    email        citext not null unique,      -- lookup and invites only
    display_name text,
    created_at   timestamptz not null default now(),
    deleted_at   timestamptz
);

-- One row per external login. The IdP's `sub` is a foreign key, never our identity:
-- this is what makes the IdP swappable and multi-IdP linking free.
create table identities (
    id            uuid primary key default gen_random_uuid(),
    user_id       uuid not null references users(id) on delete cascade,
    issuer        text not null,
    subject       text not null,
    last_login_at timestamptz,
    created_at    timestamptz not null default now(),
    unique (issuer, subject)
);

create table orgs (
    id         uuid primary key default gen_random_uuid(),
    slug       citext not null unique,
    name       text not null,
    created_at timestamptz not null default now()
);

create table memberships (
    org_id   uuid not null references orgs(id) on delete cascade,
    user_id  uuid not null references users(id) on delete cascade,
    role     text not null check (role in ('owner','admin','member')),
    added_at timestamptz not null default now(),
    primary key (org_id, user_id)
);
