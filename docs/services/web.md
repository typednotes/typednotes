# `web` — frontend

**Language:** Dioxus 0.7 (web + desktop)

## 1. Purpose

The user interface: OIDC login handoff, connecting third-party services, triggering and
watching agent runs, billing and credit purchase.

Also hosts the **login UI** that [`idp`](idp.md) redirects to for its challenge handoff —
see `idp` §3. That is the one place where frontend code sits on a security-critical path.

## 2. The honest framing

**This service has no proof story, and should not pretend to one.** Its correctness is
conventional: types from the Dioxus/Rust side, component tests, and manual review. Nothing
here belongs in tiers 1–3 of `../proof-strategy.md`.

Listing it is still worth it, because two of its invariants are load-bearing and easy to
violate by accident (§4).

## 3. Dependencies

Dioxus 0.7 — `Router` with a `Routable` enum, `use_signal` / `use_memo` / `use_resource`,
`use_server_future` on any hydrated path. Components from `packages/ui/src/components/`
in preference to hand-rolled HTML, per `AGENTS.md`.

## 4. Invariants that actually matter

Neither is a proof; both are review rules.

1. **No third-party credential ever reaches the client.** Access tokens and refresh tokens
   live in [`secrets`](secrets.md) and are used only inside [`broker`](broker.md). The
   frontend sees connection *status*, never a token. A "just for debugging" endpoint that
   returns one is the failure mode to watch for.
2. **The client never mints or attenuates a warrant.** Warrants are minted in
   [`core`](core.md) and attenuated in [`agent`](agent.md). A warrant in browser-reachable
   code is attenuable by the user's browser — which is not a vulnerability by itself, since
   attenuation only narrows, but it leaks the authority model and invites the first mistake.

## 5. What is not proven

Everything. Specifically worth testing rather than reasoning about:

- **Hydration parity** — server and client first render must match, or state desynchronises.
  Use `use_server_future`, and keep browser-only access (`localStorage`) inside `use_effect`.
- **The OIDC callback path** — `state` and `nonce` handling, and that an error response is
  never treated as success.
- **Credit purchase flow** — the client must not assume a purchase succeeded before the
  Stripe webhook lands in [`ledger`](ledger.md). Optimistic UI here shows users credits they
  do not have.

## 6. Open questions

- **Does the login UI ship inside `idp` or inside `web`?** Inside `idp` keeps the
  authentication surface in one deployable and avoids a cross-origin challenge handoff;
  inside `web` gives a consistent design system. Leaning `idp`.
- Desktop build: does it use the same OIDC flow, or the device authorization grant?
