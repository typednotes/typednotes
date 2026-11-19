# SPEC — api

`api` defines the client-facing data types and full-stack server functions for the
Typednotes core. See the [package README](README.md) for the code map and the
[core service contract](../../docs/services/core.md) for ownership and persistence.

## Interface and dependencies

- Shared models and server-function signatures are exported from [`src/lib.rs`](src/lib.rs).
- With the `server` feature, `auth_routes()` provides the routes to merge into the
  web server, and `start_background()` starts the source scheduler from the server runtime.
- The server uses Postgres for core state and delegates provider calls and secret
  handling as described in the [connection contract](../../docs/connections.md).

## Guarantees

- **S1** (unverified) — `list_orgs()` resolves the current session and lists only
  organizations in which the signed-in user has a membership. An unauthenticated
  caller gets an error rather than an organization list.
- **S2** (unverified) — Connection responses do not return stored provider tokens
  or API keys to the client.
- **S3** (unverified) — The `server` feature gates the server modules, including
  database access and server-side credential handling.

## Non-goals

This package does not apply database migrations or implement the broker, secret
vault, code generator, or Lean execution runtime.
