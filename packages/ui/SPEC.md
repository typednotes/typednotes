# SPEC — ui

`ui` provides the reusable Dioxus views and components for Typednotes. The
launching packages own their route trees and select which views to mount. See the
[package README](README.md) and the [web service contract](../../docs/services/web.md).

## Interface and dependencies

- [`src/lib.rs`](src/lib.rs) exports design-system components, shared app views,
  and the `COMPONENTS_THEME` and `APP_THEME` assets.
- Hosts include `COMPONENTS_THEME` once at the app root, followed by `APP_THEME`.
  Views that need server data use the shared `api` functions.

## Guarantees

- **S1** (unverified) — Web and native hosts can import `Navbar`, `Logo`, and
  `OrgsPanel` through the same public API without depending on another host.
- **S2** (unverified) — For a `ServerFnError::ServerError` passed to the shared
  error formatter, the returned text is the server's message without the
  transport wrapper.

## Non-goals

UI availability and validation do not grant authority. The package does not
enforce server-side access to organizations, credentials, or connectors.
