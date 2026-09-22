# `ledger` — usage events and credits

**Language:** Lean 4 + `linen` · **Deployed inside** [`core`](core.md), documented separately
because its invariants are distinct

## 1. Purpose

Record what was consumed and what it cost, and hold the credit balance that bounds spend.
Written by [`broker`](broker.md) at the egress and inference call sites; rolled up and
pushed to Stripe.

**Not responsible for:** invoices, VAT, or e-invoicing — all bought, see
`../architecture.md` §5.3–5.5. **Stripe is never the source of truth for usage.**

## 2. Dependencies

`Database/{PostgreSQL,SQL/Pool}`, `Data/Json`, `Time/{Calendar,Clock}`.
`Control/Concurrent/Green` for the expired-hold sweeper.

**Deliberately unused:** `Data/Rat`, `Data/Fixed`, `Data/Float`. Money is integers.

## 3. A different balance of tiers

Unlike `broker` and `idp`, **most of this service's safety is tier 4, not tier 1–3.** The
no-double-spend property is a concurrency property over Postgres, not over Lean's heap: two
containers, two concurrent requests, one org. Lean's types see one process. Serverless
hosting makes this sharper, not softer — there is no in-memory balance to protect.

The division, and it is worth stating so nobody later "improves" it:

> **Lean makes the arithmetic and the lifecycle correct. Postgres makes the concurrency
> correct.** The pure core is only trustworthy because it never tries to own the balance.

## 4. State

```sql
create table usage_events (
    id              uuid primary key default gen_random_uuid(),
    org_id          uuid not null references orgs(id),
    user_id         uuid references users(id),      -- null for org-level system usage
    run_id          uuid,
    event_type      text not null,                  -- 'inference' | 'egress' | ...
    provider        text not null,                  -- 'mistral' | 'baseten' | 'notion'
    model           text,
    input_tokens    bigint,
    output_tokens   bigint,
    cost_micros     bigint not null default 0,      -- what WE paid
    price_micros    bigint not null default 0,      -- what we CHARGED
    credits         bigint not null default 0,      -- billing unit
    idempotency_key text not null,
    occurred_at     timestamptz not null default now(),
    unique (idempotency_key)
);

create index on usage_events (org_id, occurred_at);
create index on usage_events (run_id);

-- Append-only. Balance is derived, never stored.
create table credit_ledger (
    id          uuid primary key default gen_random_uuid(),
    org_id      uuid not null references orgs(id),
    delta       bigint not null,               -- sign set by `reason`
    reason      text not null,
    run_id      uuid,
    usage_event uuid references usage_events(id),
    created_at  timestamptz not null default now()
);

create index on credit_ledger (org_id, created_at);

-- In-flight holds, so concurrent runs cannot each spend the same balance.
create table credit_holds (
    id         uuid primary key default gen_random_uuid(),
    org_id     uuid not null references orgs(id),
    run_id     uuid not null,
    amount     bigint not null,
    state      text not null check (state in ('held','settled','released')),
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);

create index on credit_holds (org_id) where state = 'held';
```

Four things painful to retrofit, all visible above:

1. **`unique (idempotency_key)`.** Agent runs retry. Without it we double-bill and learn
   about it from a customer.
2. **`cost_micros` and `price_micros` are separate.** We route across Baseten, Mistral and
   Scaleway, whose prices differ. Margin must be visible per run.
3. **Bill in credits, not raw tokens.** A pricing page saying "tokens" while we silently
   re-route makes margin swing with routing decisions and the page fiction.
4. **Cost basis recorded at call time.** Provider prices change; history must not be
   re-derived from a current price list.

## 5. Core types

```lean
abbrev Credits := Nat        -- counts; negative is unrepresentable
abbrev Micros  := Int        -- signed money
```

`Credits` is the same type as `broker`'s `budget` caveat, so spend authority and ledger
arithmetic cannot drift.

Sign comes from the constructor, never from the data:

```lean
inductive Entry
  | purchase    (amount : Credits) (stripe : StripeEventId)   -- +
  | grant       (amount : Credits) (reason : GrantReason)      -- +
  | usage       (amount : Credits) (event  : UsageEventId)     -- −
  | usageRefund (amount : Credits) (entry  : EntryId)          -- +
  | chargeback  (amount : Credits) (stripe : StripeEventId)    -- −

def Entry.delta : Entry → Int
  | .purchase a _ | .grant a _ | .usageRefund a _ =>  (a : Int)
  | .usage a _    | .chargeback a _               => -(a : Int)
```

This forced a disambiguation the SQL `reason` column was hiding: `'refund'` could mean
returning credits for bad usage or clawing back a purchase — opposite signs.

```lean
def balance (es : List Entry) : Int := es.foldl (fun b e => b + e.delta) 0
```

Holds get the phase-index treatment:

```lean
inductive HoldPhase | held | settled | released

inductive HoldStep : HoldPhase → HoldPhase → Type
  | settle  : HoldStep .held .settled
  | release : HoldStep .held .released
  -- nothing leaves .settled or .released → settle-once by construction
```

Idempotency keys are **derived, not generated** — the highest-value type here:

```lean
structure IdempotencyKey where
  private mk ::
  value : String

/-- A function of the authorized request, so a retry produces the same key. -/
def IdempotencyKey.ofReserved (r : Reserved req) (attempt : Nat) : IdempotencyKey
```

There is no constructor that lets a retry path invent a fresh random key — which is
precisely how double-billing happens.

## 6. What is proven

**Tier 1 · Type:**

- a negative purchase is unrepresentable (`Credits := Nat`, sign from the constructor);
- a hold cannot settle twice, or settle after release (no constructor leaves those phases);
- credits, micros and token counts cannot be mixed.

**Tier 2 · Witness** — conservation, which is what actually enforces the warrant's `budget`
caveat at the point of spending rather than trusting `broker` to re-check:

```lean
structure Settlement (h : Hold) where
  private mk ::
  actual : Credits
  within : actual ≤ h.amount
```

**Tier 3 · Theorem:**

```lean
theorem balance_append (es fs : List Entry) :
    balance (es ++ fs) = balance es + balance fs
```

Nearly trivial, and it is the theorem that **licenses snapshotting** — cache a balance at a
cutoff, fold only newer entries, and know it agrees with a full recompute. Without it,
caching is a guess. It also makes the Stripe reconciliation incremental rather than a
full-history scan.

## 7. What is not proven

**Tier 4 · Constraint — most of the real safety.** Balance check and hold insert must be
**one atomic statement**, never a read then a write:

```sql
insert into credit_holds (org_id, run_id, amount, state, expires_at)
select $1, $2, $3, 'held', now() + interval '15 minutes'
where (
  select coalesce(sum(delta), 0) from credit_ledger where org_id = $1
) - (
  select coalesce(sum(amount), 0) from credit_holds
   where org_id = $1 and state = 'held'
) >= $3
returning id;
```

Zero rows returned means denied — no race, no advisory lock. And idempotency is enforced by
the unique index: **catching the unique violation *is* the idempotent path.** An
application-level "have I seen this key?" check is the bug.

**Tier 5 · Test:** the sweeper releases expired holds exactly once; Stripe webhooks are
idempotent on the Stripe event id; rollups agree with Stripe (a reconciliation job that
alerts on divergence, since our ledger stays authoritative).

**Prepaid, not postpaid** — a policy choice, not a proof. A runaway agent loop on postpaid
metering is a bill we cannot collect from a customer who is furious about it. Two caps, both
enforced in `broker`: a per-run cap carried in the warrant, and a per-org balance refused at
zero.

## 8. Where this service is unusually comfortable

`../proof-strategy.md` lists witness reconstruction as a structural tax: witnesses do not
survive Postgres, so every request rebuilds invariants from rows. **For the ledger that tax
is zero** — it already *is* an append-only log, and `balance` already *is* the fold. The
reconstruction function and the domain function are the same function.

This is the one component where the event-sourced shape is not a design choice made to get
proofs. It is just what a ledger is.

## 9. Open questions

- **One credit = one what?** It must survive provider re-routing without changing the
  customer's mental model. Undecided, and it blocks the pricing page.
- **Hold expiry window.** 15 minutes is a guess; it should exceed the longest plausible
  run, which depends on the answer to `core`'s warrant-lifetime question.
- **Free tier / trial grants** — a `grant` entry with an expiry? The current `Entry` type
  has no expiring credits, and adding them later changes `balance` from a fold into a
  time-dependent function. Decide before launch.
