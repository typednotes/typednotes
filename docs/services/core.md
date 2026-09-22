# `core` — users, orgs, resources, warrant minting

**Language:** Lean 4 + `linen` · **Deploys** [`ledger`](ledger.md) as a module

## 1. Purpose

Own the identity *data* model and all authorization decisions, and mint the warrants that
give [`agent`](agent.md) bounded authority. This is the service that knows who may do what.

**Not responsible for:** authenticating humans ([`idp`](idp.md)), reaching third parties
([`broker`](broker.md)), or storing third-party credentials ([`secrets`](secrets.md)).

## 2. Dependencies

`Network/WebApp`, `Database/{PostgreSQL,SQL/Pool}`, `Crypto/JOSE/{JWK,JWS,JWT}` for
verifying `idp` tokens offline, `Crypto/JOSE/FFI.hmac` for minting warrant tags,
`Data/{Json,Parser}`, `Time/Calendar`.

**Gaps:** no UUID module — mint ids with Postgres `gen_random_uuid()`, as the schema
already does.

## 3. Interface

Consumed by [`web`](web.md) for everything the user does, and by [`broker`](broker.md)
only for the root key and hold settlement. `core` never calls `broker`; the dependency runs
one way, which is what keeps the egress chokepoint a chokepoint.

Mints warrants on run start (§5). Verifies `idp` id_tokens against JWKS — no call-home.

## 4. State

```sql
create table users (
    id           uuid primary key default gen_random_uuid(),
    email        citext not null unique,      -- lookup and invites only
    display_name text,
    created_at   timestamptz not null default now(),
    deleted_at   timestamptz
);

-- One row per external login. The IdP's `sub` is a foreign key, never our identity:
-- this is what makes the IdP swappable and multi-IdP linking free.
create table identities (
    id            uuid primary key default gen_random_uuid(),
    user_id       uuid not null references users(id) on delete cascade,
    issuer        text not null,
    subject       text not null,
    last_login_at timestamptz,
    created_at    timestamptz not null default now(),
    unique (issuer, subject)
);

create table orgs (
    id         uuid primary key default gen_random_uuid(),
    slug       citext not null unique,
    name       text not null,
    created_at timestamptz not null default now()
);

create table memberships (
    org_id   uuid not null references orgs(id) on delete cascade,
    user_id  uuid not null references users(id) on delete cascade,
    role     text not null check (role in ('owner','admin','member')),
    added_at timestamptz not null default now(),
    primary key (org_id, user_id)
);
```

Every other table in the system keys off `users.id` / `orgs.id`. Nothing downstream ever
sees an issuer or a `sub`.

## 5. Core types

Roles form a total order, which is the whole point of modelling them as a lattice rather
than as strings:

```lean
inductive Role | member | admin | owner
  deriving DecidableEq, Repr

instance : LE Role := ⟨fun a b => a.rank ≤ b.rank⟩
```

Access is a decidable predicate over data, not an `IO` call:

```lean
def canAccess (m : Membership) (r : Resource) (a : Action) : Prop
instance : Decidable (canAccess m r a)
```

Minting produces a warrant whose caveats are bounded by what the member actually holds:

```lean
def mint (m : Membership) (req : RunRequest) (rootKey : RootKey)
    : IO (Except Denial Warrant)
```

`Warrant` and `Caveat` are defined in [`broker`](broker.md) §5 and shared — one definition,
two consumers, so minting and verification cannot drift.

## 6. What is proven

**Tier 1 · Type:**

- role comparisons go through `LE Role`, so `"admin" > "owner"` string bugs are
  unrepresentable;
- `Action` and `Provider` are closed sums — no stringly-typed capability can be constructed.

**Tier 3 · Theorem** — two, and the first is the most important theorem in the system:

```lean
/-- Minting never grants more than the member holds.
    The soundness half of delegation; `attenuate_monotone` in broker is the other half. -/
theorem mint_sound (m : Membership) (req : RunRequest) (w : Warrant) :
    mint m req = .ok w → ∀ r, w.permits r → canAccess m r.resource r.action
```

```lean
/-- Authority is monotone in role: a higher role can do anything a lower role can.
    Catches permission checks that accidentally exclude owners — a real bug class. -/
theorem canAccess_mono (m₁ m₂ : Membership) (h : m₁.role ≤ m₂.role)
    (hsame : m₁.orgId = m₂.orgId) :
    canAccess m₁ r a → canAccess m₂ r a
```

Together with `broker`'s `attenuate_monotone`, these say: a warrant starts no wider than
the user's own authority, and can only narrow from there. That is the delegation guarantee,
and it holds across two services because both use the same `Warrant` definition.

## 7. What is not proven

**Tier 4 · Constraint** — every one of these is a Postgres property, not a Lean one,
because the state is shared across containers:

- `(issuer, subject)` maps to at most one user — the unique index, not a check;
- a user has at most one membership per org — the composite primary key;
- **membership revoked mid-run.** `mint_sound` holds at minting time only. A warrant
  outliving a revocation is the known hole. Mitigated by short warrant lifetimes plus a
  revocation check in `broker` on each egress call; not by a proof.

**Tier 5 · Test:**

- account merge and soft-delete semantics — `deleted_at` interacts with the unique index on
  `email` in ways worth property-testing rather than reasoning about;
- tenant isolation: the invariant is that no query returns rows across `org_id`, which is a
  property of the *queries*, not of the types. Property-test every read path.

**Unmitigated:** there is no row-level security. Tenant isolation rests on every query
being written correctly. If the resource graph gets relationship-heavy, revisit
Zanzibar-style authorization (SpiceDB, OpenFGA) — but adopting it early is a large tax for
little return.

## 8. Open questions

- **Warrant lifetime.** Minutes is right for containment, but long runs then need
  re-minting mid-run. Does `broker` refresh against a run record, or does `core` issue a
  chain of short warrants up front? The two have different blast radii and this is
  currently undecided.
- **Cross-org resources.** Can a resource belong to two orgs? If yes, revisit
  authorization modelling before building on the current predicate.
- Does `core` stay Lean? Its logic is decision-shaped and suits it. Unlike `broker`, it has
  no throughput risk worth measuring first.
