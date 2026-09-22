# `idp` — identity provider

**Language:** Lean 4 + `linen` · **Decision:** [`../architecture.md`](../architecture.md) §3.1

## 1. Purpose

Authenticate humans and issue OIDC tokens. Two modules in one binary, split because they
have opposite economics:

- **engine** — the OAuth 2.0 / OIDC protocol core. Spec-frozen, zero customisation value,
  pure state machine.
- **login** — registration, passkeys, MFA, recovery, sessions. Our product surface,
  expected to change often.

**Not responsible for:** the user/org model or resource ownership (that is [`core`](core.md)),
third-party credentials ([`secrets`](secrets.md)), or authorization decisions.

## 2. Dependencies

Available in `linen`: `Network/{WebApp,HTTP,URI,Socket}`, `Database/PostgreSQL`,
`Crypto/JOSE/{JWK,JWS,JWT,Types}`, `Web/Cookie`, `Data/{Parser,Base64,Json}`,
`Crypto/SecureRandom`. `Network/OAuth2` exists as a client and is a useful reference for
the grant shapes.

Crypto is FFI: `Crypto/JOSE/FFI.lean` → `ffi/jose.c` → OpenSSL EVP. Tier-0 axioms.

**Gaps to build**, in dependency order:

| Gap | Impact | Plan |
|---|---|---|
| **No CBOR, no COSE** | blocks passkeys | build in `linen`; pure byte parsing, the best tier-1/3 target in the system. (`Linen/CDP/Domains/WebAuthn.lean` is the Chrome DevTools domain for *driving* WebAuthn in tests — not a server verifier.) |
| **No password hashing** (no Argon2/bcrypt/scrypt/PBKDF2) | no password accounts | **passkey-only**, as rauthy does. Turns a gap into a design decision. |
| **`ES*` signing absent** (`JWS.lean`: RSA only) | id_tokens are RS256 | acceptable — RS256 is the most compatible. ES256 via the existing EVP shim later. |
| **No TOTP** | no fallback second factor | small; build only if passkey-only proves too strict. |
| `Network/TLS` thin | low | serverless platform terminates TLS; serve plain HTTP behind it. |

**Read first:** [rauthy](https://github.com/sebadob/rauthy) (Rust, Apache-2.0) for the
closest design; [Kanidm](https://github.com/kanidm/kanidm) (MPL-2.0) for authentication
policy and attestation modelling; `node-oidc-provider`'s configuration docs as a
"what am I missing" checklist.

## 3. Interface

Standard OIDC: `/.well-known/openid-configuration`, `/authorize`, `/token`, `/jwks.json`,
`/revoke`, `/introspect`, `/logout`. Other services verify id_tokens offline against JWKS —
no call-home on any hot path.

Internally, engine and login meet at a challenge handoff, so login never mints tokens and
the engine never learns what a passkey is:

```
client ──▶ /authorize   (engine: persist request, emit login_challenge)
              ▼
        login module + Dioxus UI  ── authenticate by any means
              │  accept { challenge, user_id, acr, amr := [passkey] }
              ▼
        engine resumes ──▶ code ──▶ /token
```

This keeps the engine replaceable by a certified off-the-shelf OP without discarding the
login work, should §3.1 ever be reversed.

## 4. State

Postgres. `users` / `identities` / `orgs` / `memberships` are owned by [`core`](core.md) —
`idp` reads them and writes `identities.last_login_at`. It owns its own protocol tables:
pending authorization requests, issued codes, refresh-token families, sessions, registered
clients, credentials (passkey public keys).

Engine state is an **append-only event log** with state as a fold, per the reconstruction
pattern in `../proof-strategy.md`: on serverless containers there is no memory shared
between `/authorize` and `/token`, so every request rebuilds its invariants from rows.
Making the log authoritative means reconstruction *is* the verified `step`.

## 5. Core types

Pure, total, no `IO`:

```lean
def step : State → Event → State × List Effect
```

Illegal transitions are unrepresentable rather than rejected:

```lean
inductive Phase | init | authenticated | codeIssued | codeRedeemed | revoked

inductive Step : Phase → Phase → Type where
  | authenticate : Step .init          .authenticated
  | issueCode    : Step .authenticated .codeIssued
  | redeem       : Step .codeIssued    .codeRedeemed
  -- no `Step .codeRedeemed _` exists → RFC 6749 §4.1.2 single-use holds by construction
```

Token construction takes the RFC's preconditions as arguments:

```lean
structure AccessToken (req : TokenRequest) where
  private mk ::
  grant    : Grant
  pkceOk   : PkceVerified req grant
  clientOk : req.clientId = grant.clientId
  acrOk    : req.requiredAcr ≤ grant.acr
  fresh    : grant.redemptions = 0
```

Algorithm confusion becomes a type error:

```lean
inductive Alg | RS256 | PS256          -- no `none` constructor exists
structure Key (a : Alg)
def sign   : Key a → Payload → Sig a
def verify : Key a → Sig a → Payload → Bool
```

## 6. What is proven

**Tier 1 · Type** — no theorem needed:

- authorization codes are single-use (no constructor leaves `.codeRedeemed`);
- `alg = none` is unrepresentable, and key/algorithm confusion is a type error;
- `redirect_uri` is `{u : Uri // u ∈ client.registered}` — an unregistered target cannot be
  constructed, retiring a whole CVE class.

**Tier 2 · Witness** — proof is a constructor argument:

- PKCE binding, client binding, and the `acr` floor on every issued token.

**Tier 3 · Theorem** — genuinely quantifies over traces, so it cannot be a type:

- replaying a refresh token revokes its **entire family**;
- no token is issued without a prior authentication event in the trace.

Index the **engine** hard and leave the **login** module conventionally typed: indexed
types propagate, which is a fair price against a frozen RFC and a bad one against UX that
changes weekly. The module boundary in §3 already falls where this line belongs.

## 7. What is not proven

**Tier 5 · Test — and this is the load-bearing one.** We cannot prove our formalization
reads the RFCs correctly. Run the
[OpenID Foundation conformance suite](https://openid.net/certification/) in CI from the
first week `/authorize` responds: it is free, self-serviceable and language-agnostic. It is
the *only* instrument that tests the formalization rather than testing against it, and it
is also the answer to the auditability cost of a Lean IdP few people can review.

Proofs and conformance cover disjoint things. Neither substitutes.

**Tier 6 · Measure:**

- constant-time secret comparison — route through the OpenSSL shim (`CRYPTO_memcmp`);
- timing uniformity on login, registration and recovery — fixed-delay responses;
- enumeration resistance — uniform response bodies, reviewed by hand.

**Unproven and unmitigated:** the passkey attestation parser is new code on the critical
path. It is the best tier-1/3 target available, which is the argument for writing it
carefully, not for trusting it early.

## 8. Open questions

- **Passkey-only at launch?** Removes password hashing, password recovery and most
  enumeration surface, but loses some users — and passkey-only *recovery* is harder than
  password recovery, not easier. Account recovery is the top takeover vector either way.
- **Who owns `sessions`** when `idp` and `core` are separate deployments but share a
  database?
- **Client registration** — static config, or dynamic registration (RFC 7591)? Static
  until there is a second consumer.
