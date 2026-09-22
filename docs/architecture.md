# Typednotes — Architecture

**Status:** draft · **Last updated:** 2026-09-22

This document records the architectural decisions for Typednotes: a system where a
server-side agent acts on resources **on behalf of** users.

It covers two questions in particular:

1. Do we build our own identity provider (a "Keycloak in Rust"), or manage users in
   the main app?
2. How do we handle metering and invoicing?

Short answers: **neither** — identity splits into three concerns that belong in three
different places; and **build the usage ledger, buy the billing and tax engine**.

---

## 0. Documents

This file holds **system-level decisions**: what we build, what we buy, and why. Per-service
detail — types, schemas, and the specific properties each service proves — lives alongside it:

- [`proof-strategy.md`](proof-strategy.md) — the six proof tiers, cross-cutting patterns, and
  an honest account of what "provably correct" can and cannot mean here. **Read this first.**
- [`services/`](services/) — one document per service, all answering the same eight questions:
  [`idp`](services/idp.md) · [`core`](services/core.md) · [`broker`](services/broker.md) ·
  [`ledger`](services/ledger.md) · [`agent`](services/agent.md) ·
  [`secrets`](services/secrets.md) · [`web`](services/web.md)

Schemas and Lean types are stated **once**, in the owning service document. Where this file
used to carry them, it now points.

---

## 1. Context and constraints

| | |
|---|---|
| Languages | Rust (services), Lean 4 (agent, policy specification) |
| Frontend | Dioxus 0.7 (web + desktop) |
| Hosting | Serverless containers (scale-to-zero), PostgreSQL |
| Infra as code | [`typednotes/infra`](https://github.com/typednotes/infra) |
| Lean 4 stdlib | [`typednotes/linen`](https://github.com/typednotes/linen) |
| Secrets | [`typednotes/secrets`](https://github.com/typednotes/secrets) — OpenBao-like vault, in Rust |
| Inference | Baseten, Mistral, Scaleway (routed) |

Constraints that drive the decisions below:

- **Scale-to-zero containers.** Anything on the login critical path must have a fast
  cold start. This effectively rules out a JVM-based IdP.
- **Likely EU data residency.** Scaleway and Mistral suggest EU-first. Identity and
  billing vendors must offer EU hosting.
- **The agent is autonomous and acts on user-owned resources.** Authority must be
  explicit, bounded and auditable — never ambient.
- **Small team.** Every component we build must be one that no vendor sells well.

---

## 2. Service decomposition

```
                    ┌──────────────┐
      user ────────▶│  Dioxus app  │
                    └──────┬───────┘
                           │ OIDC (auth code + PKCE)
                    ┌──────▼───────────────────────┐
                    │  idp (Lean 4 + linen)        │  humans, passkeys,
                    │   · engine — OIDC/OAuth core │  MFA, recovery
                    │   · login  — credentials, UX │
                    └──────┬───────────────────────┘
                           │ id_token
                    ┌──────▼───────────────────────┐
                    │  core (Rust)                 │  users, orgs, resources,
                    │    · mints warrants          │  billing state
                    └──────┬───────────────────────┘
                           │ warrant (Biscuit)
                    ┌──────▼───────────────────────┐
                    │  agent (Lean 4)              │  planning, reasoning
                    └──────┬───────────────────────┘
                           │ warrant (attenuated)
              ┌────────────▼────────────┐   ┌─────────────────────┐
              │  broker (Rust)          │──▶│  secrets (vault)    │
              │   · verifies warrant    │   └─────────────────────┘
              │   · holds budget        │
              │   · emits usage events  │──▶ third-party APIs
              │   · audit log           │──▶ inference providers
              └─────────────────────────┘
```

`broker` is the single egress chokepoint. Nothing reaches a third party or an
inference provider except through it. This one property is what makes authority,
auditing, spend control and metering all tractable at once.

---

## 3. Identity: split the three concerns

"Keycloak or the main app?" is hard to answer because it conflates three concerns.
Keycloak claims all three and only does the first one well.

| Concern | Decision | Where |
|---|---|---|
| **(a)** Authenticating humans — passkeys, MFA, recovery, social login | **Build** (§3.1) | `idp` — Lean 4 + `linen` |
| **(b)** Users, orgs, roles, resource ownership | **Build** | Our PostgreSQL |
| **(c)** Third-party credentials for "connect to many services" | **Built** | `typednotes/secrets` + `broker` |

Keycloak's roles/groups model is too coarse for (b), and using it would mean joining
our resource graph against a system that is not our database. Its identity brokering
covers (c), but awkwardly — and `typednotes/secrets` is already the correct home for
those refresh tokens.

**We are building (a) ourselves, in Lean 4.** The decision and its reasoning are in
§3.1; the split that makes it tractable is in §3.2; what we prove is in §3.5.

### 3.1 Decision: build the IdP in Lean 4 on `linen`

We are building it, and in Lean 4 rather than Rust. The reasoning:

**`linen` already carries the plumbing.** `Network/HTTP{,2,3}`, `QUIC`, `WebSockets`,
`WebApp`, `Socket`, `URI`, `Database/PostgreSQL`, `Crypto/JOSE/{JWK,JWS,JWT}`,
`Network/OAuth2`, `Web/Cookie`, `Data/{Parser,Base64,Json}` — plus a PostgREST port that
is already an HTTP + Postgres + JWT service end to end. The usual "no server ecosystem
in Lean" objection does not apply to us.

**We are not choosing between one language and two.** `Crypto/JOSE/FFI.lean` is
`@[extern]` over `ffi/jose.c` against OpenSSL's EVP API. linen is Lean + C today. The
only open question is which second language holds the crypto shim, and that is already
answered: C/OpenSSL.

**The proof boundary is the same in any host language.** Lean's `IO` is opaque, and the
OpenSSL FFI declarations are `opaque … : IO ByteArray` — i.e. *axioms*. Nothing about
the crypto is provable from Lean regardless of what surrounds it. What is provable is
the pure decision core. That is true of a Rust implementation too, so the choice of
language does not move the boundary — it only decides how much machinery sits at it.

**Therefore Lean-native beats Rust + Aeneas.** [Aeneas](https://github.com/AeneasVerif/aeneas)
translates Rust to Lean 4 for verification and is the serious alternative: write Rust,
prove in Lean. But for the same theorems it means maintaining Rust, plus generated Lean,
plus hand proofs, and it puts Charon and Aeneas inside our trust base. The September 2026
Aeneas/Rust/Lean report reports **237 KLOC of Lean for 16.7 KLOC of Rust** (~10:1) — that
is post-quantum crypto, our worst case, but it is not a ratio to pay for a translation we
do not need. Writing Lean directly means the theorems are about the code that ships.

**The decisive reason is amortization.** The trace-reasoning layer needed for the OIDC
state machine is the same one needed for warrant attenuation monotonicity (§4.4) and the
agent's policy reasoning. One proof infrastructure in `linen`, three consumers.

Revisit if and only if: passkey/attestation work stalls for more than a quarter, or
conformance (§3.6) cannot be reached.

### 3.2 Structure: protocol engine vs. login, in one binary

Copy Ory's Hydra/Kratos separation as a **module boundary**, not a service boundary. The
two halves have opposite economics:

| | Protocol engine | Login & credentials |
|---|---|---|
| Contains | authorize, code issuance, PKCE, token endpoint, refresh rotation, JWKS, discovery, revocation, introspection, logout | registration, passkeys, MFA, recovery, verification, sessions |
| Customisation value | **zero** — deviating from the RFCs is a bug | **high** — our product surface |
| Shape | pure total state machine (§3.5) | `IO`, Postgres, mail |

They meet at a challenge handoff, so the login UI never mints tokens and the engine never
learns what a passkey is:

```
client ──▶ /authorize  (engine: store request, emit login_challenge)
              ▼
        login module / Dioxus UI  ── authenticate by any means
              │  accept { challenge, user_id, acr, amr := [passkey] }
              ▼
        engine resumes ──▶ code ──▶ /token
```

This keeps the engine replaceable by a certified off-the-shelf OP if we ever reverse §3.1,
without discarding the login work.

**Read before writing:** [rauthy](https://github.com/sebadob/rauthy) (Rust, Apache-2.0) is
the closest existing design — passkey-first, Postgres, device grant — and
[Kanidm](https://github.com/kanidm/kanidm) (Rust, MPL-2.0) for its authentication-policy
and WebAuthn-attestation modelling. Read both as designs;
[node-oidc-provider](https://github.com/panva/node-oidc-provider)'s configuration docs are
the best available "what am I missing" conformance checklist.

**Out of scope**, permanently unless revisited: SAML, LDAP federation, user-storage SPI,
multi-realm (our `orgs` table does this), configurable authentication-flow DAGs, and a
separate admin SPA (we have Dioxus). That list is most of Keycloak's mass and none of our
product.

### 3.3 Own our user IDs

The fear that usually pushes teams into building their own Keycloak is lock-in. The
cure is about 30 lines of schema, not a new service: **the IdP's `sub` is a foreign key,
never our identity.**

Schema: [`services/core.md`](services/core.md) §4 — `users`, `identities`, `orgs`,
`memberships`.

Every other table keys off `users.id` / `orgs.id`. Nothing downstream ever sees an issuer
or a `sub`.

### 3.4 Authorization

Start with plain ownership and role checks in Postgres. Each service verifies tokens
offline (JWKS for OIDC id_tokens, public-key verification for warrants) so there is no
call-home on the hot path.

Reach for Zanzibar-style authorization (SpiceDB, OpenFGA) **only** if the resource graph
becomes genuinely relationship-heavy with deep transitive sharing. Adopting it early is
a large operational tax for little return.

### 3.5 Encoding and proof strategy

The engine encodes RFC requirements as **types with attached witnesses**, so illegal states
are unrepresentable rather than rejected at runtime. Dependent typing pays off precisely
where a specification is frozen, and OAuth 2.0 core and OIDC are frozen.

The tier vocabulary and the cross-cutting patterns are in
[`proof-strategy.md`](proof-strategy.md); the engine's concrete types, theorems and
non-goals are in [`services/idp.md`](services/idp.md) §5–7.

One rule belongs here because it is an architectural constraint rather than a proof detail:
**index the engine, not the login module.** Indexed types propagate, which is a fair price
against a frozen RFC and a bad one against UX that changes weekly. The §3.2 module boundary
falls exactly where this line belongs.

### 3.6 Known gaps in `linen`

Four gaps sit between us and a working IdP — no CBOR/COSE (blocks passkeys), no password
hashing (argues for passkey-only), no `ES*` signing (RS256 is fine), no TOTP. Details,
impact and plan per gap: [`services/idp.md`](services/idp.md) §2.

### 3.7 Conformance is the safety net

**Run the [OpenID Foundation conformance suite](https://openid.net/certification/) against
the engine, in CI, from the first week `/authorize` responds.** It is free, self-serviceable
and language-agnostic. This is what converts "did we implement OIDC correctly?" from a
judgement call into a test run, and it is the main answer to the auditability cost of a Lean
IdP that few people can review. Self-certification also gives enterprise buyers something
concrete to point at.

Proofs and conformance cover different things and neither substitutes for the other: the
proofs cover our state machine's invariants, the suite covers our reading of the RFCs.

---

## 4. Delegation: the other thing worth building ourselves

The agent acts on resources *on behalf of* users. That delegation layer is the novel,
differentiating piece, and no vendor serves it well.

(Implementation language is open — see §8. `broker` is concurrency- and I/O-bound, which
is where Lean is weakest, so the §3.1 reasoning does not transfer automatically.)

**The rule: the agent never holds a user's long-lived credential, and never has ambient
authority.**

### 4.1 Warrants

When a user triggers an agent run, `core` mints a **warrant**: a short-lived, narrowly
scoped, auditable token stating exactly what the agent may do.

**Format: macaroon-style HMAC chains.** This supersedes an earlier decision to use
[Biscuit](https://www.biscuitsec.org/): `linen` has no Biscuit, no Datalog, no Protobuf and
**no Ed25519**, while HMAC-SHA256 is already in the JOSE shim. A macaroon chain gives the
one property that mattered — attenuation without the root key, and caveats that cannot be
removed because HMAC is not invertible.

What it gives up is third-party verification: only a root-key holder can verify. `broker` is
the sole verifier and shares a database with `core`, so that is the right trade here. If it
ever changes, adding `EVP_PKEY_ED25519` to `ffi/jose.c` is ~40 lines of C.

Caveats are a **closed inductive type**, not an open Datalog — our checks are first-order
predicates over finite sets, so evaluation is total by structural recursion and needs no
termination proof.

Full types, the attenuation theorem, and the witness chain that makes the chokepoint hold by
typing: [`services/broker.md`](services/broker.md) §3–6.

### 4.2 The broker chokepoint

`broker` is the only component that can reach outside. On each request it:

1. Verifies the warrant offline (signature, expiry, checks).
2. Confirms the requested call is within `capability` and `resource_set`.
3. Confirms and **reserves** budget against the org's credit balance (§5.2).
4. Fetches the third-party token from `typednotes/secrets`, scoped to
   `(user_id, provider, connection_id)`. **This token is never returned to the agent
   or to the client.**
5. Makes the call.
6. Writes an audit record and a usage event, then settles the budget reservation.

Side-effecting calls (sending mail, writing documents, spending money) additionally
check a per-connection policy that can require human confirmation. Because there is
exactly one chokepoint, human-in-the-loop is a feature of the architecture rather than
something retrofitted per integration.

### 4.3 Threat model: prompt injection

The agent reads user resources — emails, documents, issues — that may contain
adversarial instructions aimed at the agent itself. **There is no reliable
prompt-level defence against this.** Bounded, non-ambient authority *is* the defence:

- A hijacked agent can only act inside its warrant's capabilities and resource set.
- It cannot widen the warrant (attenuation is monotone).
- It cannot exceed the run budget.
- Every attempt is in the audit log, attributed to a `run_id`.
- Side-effecting calls can require a human.

Design consequence: warrants must be minted **narrowly per run**, from the user's actual
request. A convenience warrant covering "everything the user can do" discards the entire
protection.

### 4.4 Lean 4's role: the shared proof layer

Specify the delegation and policy semantics in Lean 4 and prove the properties we care
about:

- attenuation is monotone — no derived warrant authorises more than its parent;
- no warrant grants access outside its `resource_set`;
- total settled spend across a run never exceeds `budget_credits`.

Details: [`services/agent.md`](services/agent.md) §5 for the run-level bound,
[`services/core.md`](services/core.md) §6 for `mint_sound`.

Attenuation monotonicity is **witness-tier** in the sense of §3.5 — an attenuation should
only be constructible together with a proof that it narrows. Reuse the trace layer built
for the engine core rather than building a second one.

Keep enforcement in `broker` tested against the Lean specification with property tests
over generated warrants. Verified extraction is a worthwhile long-term
goal but must not be a prerequisite for shipping.

The agent's planning and policy reasoning is where Lean 4 earns its place in the
product; `linen` is the foundation for that.

---

## 5. Metering and invoicing

Build the ledger. Buy everything downstream of it.

### 5.1 Usage events

Append-only, written by `broker` at the egress and inference call sites. **Stripe is
never the source of truth for usage.**

Schema: [`services/ledger.md`](services/ledger.md) §4.

Four things that are painful to retrofit:

1. **`idempotency_key` on every event, enforced by a unique index.** Agent runs retry.
   Without this we will double-bill, and we will learn about it from a customer.
2. **`cost` and `price` are separate columns.** We route across Baseten, Mistral and
   Scaleway, whose prices differ. Cost is what we paid; price is what we charged. Margin
   must be visible per run, not inferred at the end of the quarter.
3. **Bill in credits, not raw tokens.** If the pricing page says "tokens" while we
   silently route between three providers, margin swings with routing decisions and the
   pricing page becomes fiction. Credits decouple our pricing from provider mechanics.
4. **Record the cost basis at call time.** Provider prices change; historical events must
   not be re-derived from a current price list.

### 5.2 Credits: prepaid, with reservations

For agent workloads this is the important decision. **A runaway loop on postpaid metering
is a bill we cannot collect from a customer who is furious about it.** Prepaid credits
with a hard check at the chokepoint bound the damage on both sides.

Schema: [`services/ledger.md`](services/ledger.md) §4 — `credit_ledger`, `credit_holds`,
and the atomic conditional insert that makes the balance check race-free.

Flow per run: `core` reserves a hold for the warrant's `budget_credits`; `broker`
decrements against the hold as calls complete; on run end the hold settles to actual
usage and the remainder is released. Expired holds are swept by a periodic job.

Two caps, both enforced at `broker`:

- **Per-run cap** — carried in the warrant. One run cannot consume a month of credits.
- **Per-org balance** — refuse at zero.

The warrant thus bounds **authority and spend with the same mechanism**, which is what
ties §4 and §5 together.

### 5.3 Billing

Roll up `usage_events` per org per period and push to **Stripe Billing** as metered
usage. Run a **reconciliation job** comparing our rollups against what Stripe recorded,
and alert on divergence. Our ledger stays authoritative; Stripe is a renderer and a
payment rail.

Credit purchases are ordinary Stripe payments that append a `purchase` row to
`credit_ledger` via webhook — idempotently, keyed on the Stripe event ID.

### 5.4 Tax — the real reason not to build this

Selling software into the EU means:

- **B2C:** VAT at the buyer's country rate, declared through the OSS one-stop-shop.
- **Cross-border B2B in the EU:** reverse charge, which requires validating the customer's
  VAT ID against VIES and retaining evidence.
- **Non-EU:** generally no VAT, with exceptions per jurisdiction.

**Stripe Tax** handles this. A **merchant-of-record** (Paddle, Polar) goes further and
assumes the liability itself at a higher take rate. For a small team, MoR is often the
right trade — revisit once volume makes the rate differential material.

### ⚠️ 5.5 French e-invoicing — verify before committing

If the billing entity is French and sells B2B in France, the e-invoicing reform
(*facturation électronique*) applies: structured invoices in Factur-X / UBL / CII, issued
through a certified **PDP** (*plateforme de dématérialisation partenaire*) rather than
direct issuance, since the public portal was scaled back to a directory and concentrator
role.

Phase-in as understood: reception obligations for all companies from September 2026, with
issuance obligations for large and mid-size companies from the same date and smaller
businesses following in 2027.

**This needs checking against the current DGFiP timetable before we commit to a billing
provider.** The schedule has slipped more than once, and this note was written with
knowledge current only to mid-2026. Confirm that whichever provider we pick is
PDP-integrated for France. This is a hard legal constraint, not an optimisation, and
migrating invoicing later is far more expensive than choosing correctly now.

---

## 6. Build / buy summary

| Component | Decision | Rationale |
|---|---|---|
| IdP protocol engine | **Build in Lean 4** | Pure state machine — provable, and the proof layer is shared with §4.4 |
| IdP login / credentials | **Build in Lean 4** | The product surface; passkey-only to start |
| Crypto primitives | **FFI to OpenSSL** | Already how `linen` works; axiomatic either way |
| Users, orgs, resource model | **Build** — Postgres | Must be joinable with our own data |
| Delegation warrants + `broker` | **Build in Rust** | The differentiator; no vendor does this well |
| Third-party token storage | **Already built** — `typednotes/secrets` | Legitimately load-bearing |
| Usage ledger + credits | **Build** | Small, and nobody sells it correctly |
| Billing, invoices, VAT, e-invoicing | **Buy** — Stripe Billing + Tax, or an MoR | Regulated, thankless, endless |
| Agent planning / policy reasoning | **Build in Lean 4** | The product |

---

## 7. Suggested sequencing

1. **Identity spine.** `users` / `identities` / `orgs` / `memberships`, plus the engine
   core (§3.5) as a pure state machine with `#guard` coverage — no `IO` yet.
2. **Engine shell + conformance.** Wire the core to `Network/WebApp` and
   `Database/PostgreSQL`; point the conformance suite (§3.7) at it in CI. Password-less
   stub login until step 3.
3. **Passkeys.** CBOR + COSE + attestation in `linen` (§3.6), then the login module and
   the Dioxus UI. Prove the parsers.
4. **Connections.** OAuth connect flow per provider; refresh tokens into
   `typednotes/secrets`, scoped `(user_id, provider, connection_id)`.
5. **`broker` v0.** Warrant minting and verification with Biscuit, plus the audit log.
   No budget yet. Route one provider through it.
6. **Agent v0.** Lean 4 agent that can only reach the world through `broker`.
7. **Metering.** `usage_events` with idempotency, then credits with holds and the two caps.
8. **Billing.** Stripe Billing + Tax, rollups, reconciliation job, webhooks into
   `credit_ledger`.
9. **Warrant proofs.** Formalise the warrant semantics on the trace layer built in step 1
   (§3.1, §4.4).

Steps 1–6 have no billing dependency, so the §5.5 e-invoicing question can be resolved
in parallel — but it must be resolved before step 8.

---

## 8. Open questions

- **Does `broker` stay in Lean 4?** `core` and `ledger` are decision-shaped and suit Lean;
  `secrets` deliberately stays Rust ([`services/secrets.md`](services/secrets.md) §2, for
  constant-time reasons that are central to a vault rather than incidental). `broker` is the
  undecided one: it is the most concurrency-bound service, and Lean's scheduler is not Tokio.
  **Settle it by load-testing a stub that sleeps 300 ms**, not by argument —
  [`services/broker.md`](services/broker.md) §7. A day's work against a rewrite.
- **Passkey-only at launch?** (§3.6) It removes password hashing, recovery-by-password and
  most enumeration surface, but it will lose some users and needs a real account-recovery
  story — which is the #1 takeover vector regardless of credential type.

- **Warrant lifetime.** Minutes is right for safety, but long agent runs then need
  re-minting mid-run. Does `broker` refresh against a run record, or does `core` issue a
  chain of short warrants up front?
- **Cross-org resources.** Does a resource ever belong to two orgs? If yes, revisit §3.4
  sooner rather than later.
- **Credit pricing basis.** One credit = one what? It must survive provider re-routing
  without changing the customer's mental model.
- **`secrets` availability.** `broker` depends on it on every egress call. What is the
  failure behaviour — fail closed, presumably, but with what caching, if any?
- **Inference provider fallback.** Does the routing decision live in the Lean agent or in
  `broker`? Putting it in `broker` keeps cost accounting in one place.
- **Data residency.** Which providers see user content, and does that need to be an
  org-level configuration for enterprise customers?
