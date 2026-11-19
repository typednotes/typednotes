# SPEC — web

`web` is the deployable Typednotes application. It owns the web route tree, app
layout, and server composition. See the [package README](README.md) and the
[web service contract](../../docs/services/web.md).

## Interface and dependencies

- [`src/main.rs`](src/main.rs) declares the routes and feature-selected entry
  points. The app composes `ui` views with `api` data and server functions.
- The `web` feature selects the browser renderer; `server` selects server rendering
  and the server-side features of `ui` and `api`.

## Guarantees

- **S1** (unverified) — The server entry point starts `api` background work and
  serves the app and `api::auth_routes()` from the same router.
- **S2** (unverified) — Project route parsing passes decoded slugs through as opaque
  text. The existing `encoded_path_segments_round_trip_as_opaque_text` test covers
  encoded spaces and percent signs but does not yet cite this clause.
- **S3** (unverified) — The `/s/:token` route passes the decoded token to the shared
  notebook view; routing alone does not grant access.

## Non-goals

The router does not decide organization membership or connector permissions and
does not execute notebooks itself.
