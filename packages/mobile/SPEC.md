# SPEC — mobile

`mobile` is the Dioxus mobile shell for the shared app UI. See the
[package README](README.md) for local usage.

## Interface and dependencies

The `mobile` feature selects the Dioxus mobile renderer. The executable entry
point is [`src/main.rs`](src/main.rs), and app components come from `ui`.

## Guarantees

- **S1** (unverified) — The mobile route tree exposes `/` as its home route and
  renders the shared organization panel there.
- **S2** (unverified) — The shell does not start the app server. Requests for
  server data depend on a separately running Typednotes server.

## Non-goals

The mobile shell does not reproduce the full web route tree and is not part
of the production deployment or the current CI build matrix.
