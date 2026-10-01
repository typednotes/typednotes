# Services

One document per service. System-level decisions live in [`../architecture.md`](https://github.com/typednotes/typednotes/blob/main/docs/architecture.md);
the shared proof vocabulary and its six tiers live in [`../proof-strategy.md`](https://github.com/typednotes/typednotes/blob/main/docs/proof-strategy.md).

| Service | Language | Role |
|---|---|---|
| [`idp`](https://github.com/typednotes/typednotes/blob/main/docs/services/idp.md) | Lean 4 + `linen` | OIDC/OAuth provider; passkey login |
| [`core`](https://github.com/typednotes/typednotes/blob/main/docs/services/core.md) | Lean 4 + `linen` | users, orgs, resources; mints warrants |
| [`broker`](https://github.com/typednotes/typednotes/blob/main/docs/services/broker.md) | Lean 4 + `linen` | sole egress chokepoint; verifies warrants, meters |
| [`ledger`](https://github.com/typednotes/typednotes/blob/main/docs/services/ledger.md) | Lean 4 + `linen` | usage events, credits, holds (deployed inside `core`) |
| [`agent`](https://github.com/typednotes/typednotes/blob/main/docs/services/agent.md) | Lean 4 + `linen` | planning and reasoning; no ambient authority |
| [`lode`](https://github.com/typednotes/typednotes/blob/main/docs/services/lode.md) | Lean 4 + `linen` | implements NL-described graph cells as Lean code in the project's repository |
| [`lun`](https://github.com/typednotes/typednotes/blob/main/docs/services/lun.md) | Lean 4 + `linen` | builds and runs the typed, reactive graphs (see [`../computations.md`](https://github.com/typednotes/typednotes/blob/main/docs/computations.md)) |
| [`secrets`](https://github.com/typednotes/typednotes/blob/main/docs/services/secrets.md) | **Rust** | vault; third-party refresh tokens |
| [`web`](https://github.com/typednotes/typednotes/blob/main/docs/services/web.md) | Dioxus 0.7 | frontend |

## Document template

Each service document answers the same questions in the same order, so they can be read
against each other:

1. **Purpose** — one paragraph, and what it is explicitly *not* responsible for.
2. **Dependencies** — which `linen` modules, which FFI, which gaps must be built.
3. **Interface** — the surface other services depend on.
4. **State** — schema, or "none".
5. **Core types** — the Lean shape of the pure core.
6. **What is proven** — by tier, per [`../proof-strategy.md`](https://github.com/typednotes/typednotes/blob/main/docs/proof-strategy.md).
7. **What is not proven** — and the named mechanism that covers it instead.
8. **Open questions.**

Section 7 is not optional. A service document that lists only its proofs is misleading
about where its risk actually sits.
