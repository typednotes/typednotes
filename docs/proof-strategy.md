# Proof strategy

**Status:** draft · **Last updated:** 2026-09-22

Shared vocabulary for the service documents in `docs/services/`. Each service states which
of its properties sit at which tier. The point of the tiering is to stop two failure modes:
claiming a proof where there is none, and hand-rolling a runtime check where a type would
have been free.

## What "provably correct" can and cannot mean here

No service in this system is provably correct end to end, and no amount of Lean changes
that. Three hard limits apply everywhere:

1. **`IO` is opaque.** Lean proofs are about pure functions. Every theorem stops at the
   boundary of the pure core; the shell that reads sockets and rows is unverified.
2. **FFI is axiomatic.** `Crypto/JOSE/FFI.lean` is `@[extern]` over `ffi/jose.c` against
   OpenSSL. From Lean those declarations are axioms. Nothing about the cryptography is
   provable, in any host language.
3. **Faithfulness is untestable by proof.** We cannot prove a formalization reads its
   specification correctly. Perfectly typed code can encode a confident misreading.

So the honest claim is narrower and still worth a lot: **the decision logic of each
service is correct by construction or by theorem, and everything else is covered by a
named mechanism that is not a proof.** The tiers below are those mechanisms.

## The six tiers

| Tier | Mechanism | Fails how |
|---|---|---|
| **1 · Type** | bad state has no constructor | compile error |
| **2 · Witness** | proof is a constructor argument | compile error at the call site |
| **3 · Theorem** | property over traces or aggregates of a pure function | `lake build` fails |
| **4 · Constraint** | Postgres uniqueness, atomicity, isolation | transaction aborts / zero rows |
| **5 · Test** | conformance, property, differential testing | CI red |
| **6 · Measure** | observed in production or a load test | alert / dashboard |

Tiers 1–3 are Lean. Tier 4 is where every property about **concurrent state** lives,
because the authority for that state is Postgres, not a Lean process — and on serverless
containers there is no in-memory continuity to protect. Tiers 5–6 cover what is true of
the system but not expressible about the code.

**Prefer the lowest tier that works.** A property at tier 1 costs nothing to maintain; the
same property at tier 5 costs a test that can rot and a reviewer who must remember it.

## Cross-cutting patterns

**Smart constructors with `private mk ::`.** The standard way to reach tier 2: a value
that certifies something is obtainable only from the function that checks it.

```lean
structure Authorized (r : Request) where
  private mk ::
  ...
```

*Review rule:* a stray `deriving` or a convenience helper that leaks a constructor
silently voids the invariant. Any new `deriving` clause on a witness type is a review stop.

**Parse, don't validate.** Requests arrive as untrusted bytes. Parse once at the edge into
a type that carries its invariants, and never re-check downstream. All security rests on
these boundary functions, not on the well-typed middle — so they are short, total, and
reviewed as carefully as the crypto.

**Witnesses do not cross Postgres.** Proofs are erased at runtime and cannot be stored, so
state reloaded from a row must have its invariants re-established. Where possible make the
table an **append-only log** and the state a fold over it, so reconstruction *is* the
verified domain function rather than a second hand-written path that can disagree with it.

**Phase-indexed state machines.** Index state by protocol phase and give the transition
relation only the legal constructors. "At most once" properties then hold by the absence of
a constructor rather than by a theorem.

**Integers only for money.** `linen` has `Data/Rat`, `Data/Fixed` and `Data/Float`. Use
none of them for stored amounts. Counts are `Nat`, signed money is `Int` micros.

## Never provable in Lean — do not try

| Property | Why | Covered by |
|---|---|---|
| Constant-time execution | not expressible; the RC runtime works against it | OpenSSL shim (`CRYPTO_memcmp`), tier 6 |
| Timing uniformity | not expressible | fixed-delay responses, tier 6 |
| Enumeration resistance | a property of responses in aggregate | uniform bodies, tier 5 review |
| Side channels | below the language | tier 6, and out of scope for a first release |
| No-double-spend under concurrency | state lives in Postgres | tier 4 |
| Faithfulness to an RFC | prose has no formal content | tier 5 conformance |
| Agent decision quality | the model is nondeterministic | containment, not correctness |

## Cost calibration

The September 2026 Aeneas/Rust/Lean report reports **237 KLOC of Lean for 16.7 KLOC of
Rust** — roughly 10:1 — to establish safety, panic-freedom and functional correctness for
post-quantum cryptography. Treat that as the **ceiling**, not the expectation: it is
bit-level numerical code with a formalized standard behind it.

Our tier-3 obligations are small, first-order and structural — set membership, list folds,
ordering on roles. They are also `#guard`-testable long before they are proved. If a proof
in this system starts to look like the ratios above, that is a signal the property belongs
at tier 4 or 5 instead.

## Service coverage, honestly

| Service | Provable core | Dominant tier | Honest ceiling |
|---|---|---|---|
| [`broker`](services/broker.md) | large | 1 · Type, 2 · Witness | throughput is tier 6 and unmeasured |
| [`ledger`](services/ledger.md) | large | 3 · Theorem, 4 · Constraint | no-double-spend is tier 4, not a proof |
| [`idp`](services/idp.md) | large (engine), small (login) | 1 · Type, 2 · Witness | RFC faithfulness is tier 5 |
| [`core`](services/core.md) | medium | 3 · Theorem | tenant isolation is tier 4 |
| [`agent`](services/agent.md) | harness only | 1 · Type | decisions unprovable; containment only |
| [`secrets`](services/secrets.md) | n/a — Rust | — | stays Rust, deliberately (§ constant-time) |
| [`web`](services/web.md) | minimal | 5 · Test | conventional testing; no proof story |
