# `broker` — egress chokepoint

**Language:** Lean 4 + `linen` (see §8 — throughput is unmeasured)

## 1. Purpose

The only component in the system that can reach the outside world. Every third-party API
call and every inference call passes through it. It verifies a warrant, reserves budget,
fetches the credential, makes the call, and writes the audit and usage records.

One chokepoint is what makes four separate things tractable at once: bounded authority,
a complete audit log, human-in-the-loop approval, and metering that cannot be bypassed.

**Not responsible for:** deciding *what* to do (that is [`agent`](agent.md)) or *who may*
do it (that is [`core`](core.md)). `broker` only enforces.

## 2. Dependencies

Available: `Network/HTTP/Client/{Connection,Redirect,Retry,Request,Response}` for outbound
calls, `Network/WebApp` inbound, `Control/Concurrent/{MVar,Chan,QSem,QSemN,Green}` and
`STM/{TVar,TMVar,TQueue}`, `Database/SQL/Pool`, `Crypto/JOSE/FFI.hmac`, and
`Cloud/Secret/{SecretsManager,ScalewaySecretManager}` as templates for a
[`secrets`](secrets.md) client. `Cloud/Protocol/ScalewayRest` helps on the inference side.

**Gaps to build:** rate limiter, per-provider circuit breaker, idempotency-key helper,
OpenTelemetry export. Retry and pooling are already there.

## 3. Format decision: macaroon-style, not Biscuit

`../architecture.md` §4.1 originally specified [Biscuit](https://www.biscuitsec.org/). It is
not available here: `linen` has no `Biscuit`, no `Datalog`, no `Protobuf`, and **no
Ed25519** — the JOSE shim has RSA sign/verify, EC *verify* only, and HMAC. Biscuit would
mean all four.

None of it is needed. HMAC-SHA256 is already in the shim, so use a macaroon-style chain:

```
s₀  = HMAC(rootKey, id)          -- minting, in core
sᵢ  = HMAC(sᵢ₋₁, caveatᵢ)        -- attenuation, needs no key
tag = sₙ
```

Attenuation works without the root key, and caveats cannot be removed because HMAC is not
invertible — which is the entire property we wanted from Biscuit. What is given up is
*third-party* verification: only a root-key holder can verify. `broker` is the sole
verifier and shares a database with `core`, so symmetric is correct here.

**Escape hatch:** if warrants ever need verifying by parties without the root key, adding
`EVP_PKEY_ED25519` to `ffi/jose.c` is ~40 lines of C, not a rewrite.

## 4. State

Writes `usage_events`, `credit_holds` and `credit_ledger` (owned by [`ledger`](ledger.md))
and an append-only `audit_log`. Reads warrants from the request, never from storage.

## 5. Core types

Biscuit's Datalog is general because it serves unknown users. Our checks are expiry,
capability membership, resource membership and budget — first-order predicates over finite
sets, no recursion, no joins. A **closed inductive** beats an open Datalog: evaluation is
total by structural recursion, so no termination proof is needed at all.

```lean
inductive Caveat
  | expiresAt  : UInt64            → Caveat
  | capability : Provider → Action → Caveat
  | resource   : ResourceId        → Caveat
  | budget     : Credits           → Caveat
  | runId      : RunId             → Caveat
  deriving DecidableEq, Repr

structure Warrant where
  id      : WarrantId              -- binds subject, actor, run
  caveats : List Caveat
  tag     : ByteArray
```

A warrant denotes the set of requests it permits; attenuation is intersection:

```lean
def Caveat.permits : Caveat → Request → Prop
  | .expiresAt t,    r => r.now < t
  | .capability p a, r => r.provider = p ∧ r.action = a
  | .resource id,    r => r.resource = id
  | .budget c,       r => r.cost ≤ c
  | .runId rid,      r => r.runId = rid

def Warrant.permits (w : Warrant) (r : Request) : Prop :=
  ∀ c ∈ w.caveats, c.permits r

def Warrant.attenuate (w : Warrant) (c : Caveat) (tag' : ByteArray) : Warrant :=
  { w with caveats := c :: w.caveats, tag := tag' }
```

The witness chain is the service's central invariant. Neither type has a public
constructor, so the only route in is the checking function:

```lean
/-- Authority to perform exactly `r`. -/
structure Authorized (r : Request) where
  private mk ::
  warrant   : Warrant
  tagOk     : VerifiedTag warrant      -- HMAC chain recomputed from rootKey
  permitted : warrant.permits r        -- from `decide`

def authorize (k : RootKey) (w : Warrant) (r : Request)
    : IO (Except Denial (Authorized r))

/-- Spend authority, separate from access authority. -/
structure Reserved (r : Request) where
  private mk ::
  authorized : Authorized r
  holdId     : HoldId

/-- Bracketed, so the hold always settles or releases — including on exception. -/
def withReservation (a : Authorized r)
    (k : Reserved r → IO (Response × Credits)) : IO (Except Denial Response)

def callProvider  : Reserved r → IO Response      -- third-party APIs
def callInference : Reserved r → IO Completion    -- Baseten / Mistral / Scaleway
```

Both egress kinds share one authorization path, which is also what makes metering uniform:
the `Reserved` value is exactly what the usage event records, so an event cannot be omitted
for a call that happened.

## 6. What is proven

**Tier 1 · Type:**

- **no outbound call is reachable without a verified warrant and a reserved budget** —
  `callProvider` and `callInference` take `Reserved r` and there is no other way to obtain
  one. This is the chokepoint property, held by typing rather than by review.
- `Caveat` is closed, so no unrecognised caveat can be constructed and silently ignored —
  the classic policy-engine failure.
- `attenuate` is the only way to extend a warrant, and it only prepends.

**Tier 3 · Theorem** — the security property of delegation, and it is nearly free:

```lean
theorem attenuate_monotone (w : Warrant) (c : Caveat) (tag' : ByteArray) (r : Request)
    : (w.attenuate c tag').permits r → w.permits r := by
  intro h c' hc'; exact h c' (List.mem_cons_of_mem _ hc')
```

An agent, or a hijacked sub-tool, cannot widen a warrant — not because we check, but
because no constructor widens. With `core`'s `mint_sound` this closes the delegation
guarantee end to end.

## 7. What is not proven

**Tier 2 review rule.** `private mk ::` is doing all the work in §5. A stray `deriving`
clause or a convenience helper that leaks a constructor voids the chokepoint property
silently. Any new `deriving` on `Authorized` or `Reserved` is a review stop.

**Tier 4 · Constraint:** budget availability, via the conditional insert in
[`ledger`](ledger.md) §7. Lean cannot see concurrent containers.

**Tier 5 · Test:** that `withReservation` releases on every exception path; that a revoked
membership is rejected (see `core` §7 — warrants outlive revocation by design).

**Tier 6 · Measure — the open risk:** Lean's scheduler is not Tokio. `broker` is I/O-bound
on third-party latency, which suits green threads and STM, and `QSemN` gives a per-provider
concurrency cap cheaply. But how the runtime behaves at a few thousand concurrent green
threads is an empirical question about Lean, not something to reason about.

**Load-test a stub `broker` that sleeps 300 ms and returns, before building on it.** One
measurement decides whether this service stays in Lean; it is a day's work against a
rewrite.

**Prompt injection is contained here, not solved.** The agent reads user resources that may
carry adversarial instructions; there is no reliable prompt-level defence. Bounded
non-ambient authority *is* the defence, which is why warrants must be minted narrowly per
run. A convenience warrant covering everything the user can do discards the entire
protection.

## 8. Open questions

- **Does `broker` stay Lean?** Answer with the load test above, not with argument. It is
  the most concurrency-bound service in the system and therefore the weakest fit.
- **Human-in-the-loop placement.** Side-effecting calls should require confirmation via a
  per-connection policy. Does the pending approval block a green thread, or suspend the run
  to a queue? The second scales; the first is far simpler.
- **Where does inference routing live** — `agent` or `broker`? Putting it in `broker` keeps
  cost accounting in one place, which argues for `broker`.
