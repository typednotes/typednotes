# Typednotes

A server-side agent that acts on resources **on behalf of** users, with bounded,
auditable authority. System decisions: [`docs/architecture.md`](docs/architecture.md);
per-service detail: [`docs/services/`](docs/services/).

This repository is the **app**: a Dioxus 0.7 fullstack web app whose server is the minimal
`core` of the architecture — users, identities, orgs and memberships in Postgres
(`docs/services/core.md` §4). Today it lists and creates orgs; everything else in the
architecture lives in sibling services:

| Service | Repository | Deployed by |
|---|---|---|
| app (`web` + `core`) | this one | [`typednotes-infra`](https://github.com/typednotes/typednotes-infra) |
| `ledger` — usage events, credits, holds | [`typednotes/ledger`](https://github.com/typednotes/ledger) | `typednotes-infra` |
| `liaison` — the broker, sole egress chokepoint | [`typednotes/liaison`](https://github.com/typednotes/liaison) | `typednotes-infra` |
| `secrets` — the vault | [`typednotes/secrets`](https://github.com/typednotes/secrets) | `typednotes-infra` |

**There is no authentication yet** — the IdP (`docs/services/idp.md`) is not built — so every
endpoint is public. Don't store anything real in a deployment of this version.

## Layout

```
packages/
  api/      server functions (`list_orgs`, `create_org`, `health`); sqlx, server-only
  ui/       shared UI: `OrgsPanel`, `Navbar`, and the dx components in `src/components/`
  web/      the deployable fullstack app (routes, SSR, wasm client)
  desktop/, mobile/   the same UI for native targets (they need a server URL; not deployed)
migrations/           the core schema, NNNN_description.sql — the one source of truth
scripts/dev-db.sh     applies migrations/ to a local database
Dockerfile            `dx bundle` → the `web` server binary + `public/`
```

## Develop

```sh
# a local Postgres, then the schema (the server never migrates itself)
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/typednotes
scripts/dev-db.sh

dx serve -p web            # http://localhost:8080
```

`cargo test -p api --features server` runs the validation tests;
`cargo check -p web --features server` and
`cargo check -p web --features web --target wasm32-unknown-unknown` check both halves.

## Migrations

`migrations/*.sql` is plain SQL and the only copy. In production,
`typednotes-infra` reads these files from GitHub at the release tag and
applies them as a declared `postgresMigrations` history (infra's
`docs/migrations.md`): reviewed as a plan before it runs, ordered before the
ledger's history (infra reads the order from the ledger's
`references orgs(id)`), and before the app container rolls out. The app's own
database identity has data rights only.

To add a migration: add `migrations/NNNN_description.sql`, tag a release, then
in `typednotes-infra` bump the app's release version and add the file to its
history — one line. **Shipped migrations are append-only**: infra refuses a
history whose applied prefix changed.

## Deploy

Tag `vX.Y.Z` and push: `.github/workflows/docker-publish.yml` publishes
`ghcr.io/typednotes/typednotes:X.Y.Z`. Then bump the app's release version in
`typednotes-infra` (it names both the image and the tag the SQL is read at)
and apply there. The container needs one variable, `DATABASE_URL`, which the fleet binds from a
composed secret.
