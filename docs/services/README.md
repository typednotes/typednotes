# Services

One document per service. System-level decisions live in [`../architecture.md`](../architecture.md);
the shared proof vocabulary and its six tiers live in [`../proof-strategy.md`](../proof-strategy.md).

| Service | Language | Role |
|---|---|---|
| [`idp`](idp.md) | Lean 4 + `linen` | OIDC/OAuth provider; passkey login |
| [`core`](core.md) | Lean 4 + `linen` | users, orgs, resources; mints warrants |
| [`broker`](broker.md) | Lean 4 + `linen` | sole egress chokepoint; verifies warrants, meters |
| [`ledger`](ledger.md) | Lean 4 + `linen` | usage events, credits, holds (deployed inside `core`) |
| [`agent`](agent.md) | Lean 4 + `linen` | planning and reasoning; no ambient authority |
| [`secrets`](secrets.md) | **Rust** | vault; third-party refresh tokens |
| [`web`](web.md) | Dioxus 0.7 | frontend |

## Document template

Each service document answers the same questions in the same order, so they can be read
against each other:

1. **Purpose** — one paragraph, and what it is explicitly *not* responsible for.
2. **Dependencies** — which `linen` modules, which FFI, which gaps must be built.
3. **Interface** — the surface other services depend on.
4. **State** — schema, or "none".
5. **Core types** — the Lean shape of the pure core.
6. **What is proven** — by tier, per `../proof-strategy.md`.
7. **What is not proven** — and the named mechanism that covers it instead.
8. **Open questions.**

Section 7 is not optional. A service document that lists only its proofs is misleading
about where its risk actually sits.
