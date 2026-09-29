# `lun` — the runtime

**Language:** Lean 4 + `linen` · **Repository:** [`typednotes/lun`](https://github.com/typednotes/lun)

## 1. Purpose

Turn a Lean project pinned at a commit into **typed services**: named functions
under declared signatures, and **reactive graphs** wiring them — registered once,
updated input by input, recomputing only what depends on them and answering with
what changed. This is the engine a typednotes notebook *is* once
[`lode`](lode.md) has implemented its cells ([`../computations.md`](../computations.md)).

**Not responsible for:** writing code (lode's, or the user's), credentials (a
private repository arrives through `liaison`, a sink's database credential is
read from the vault, never supplied by the caller), or deciding who may build
(its bearer token; the app holds it).

## 2. Dependencies

`linen` — `Control.Reactive` (graphs are its programs: acyclic by construction),
`Control.Monad.Effect` (the vetted rows below), `Database/SQL` (the
`PostgreSQL` handler), `Network/WebApp`, `Liaison.Wire`, `System.GitFn`'s
policies (the check-user-sources gap, §7). The toolchain, `git`, and a pre-built
linen package cache are in the image.

## 3. Interface

An HTTP service the app and lode call: `POST /v0/builds` (same request, same
build id — a restarted lun re-derives it from the repository), functions,
graphs-once, and **sessions** — a graph registered with its inputs, updated by
name, state stored between calls. Its README is the wire reference.

For computations, the contract additions ([`../computations.md`](../computations.md) §3–§4):

- a session carries a **user binding** `(org_id, user_id)` and a **graph
  binding** (`graph_id`), plus the graph's declared secret names and — for
  `storage` sinks — the warrants the app attaches to each update;
- a build request may declare functions whose rows include `PostgreSQL`; the
  handler is bound **per session** to the user binding's credential (vault path
  `secret/data/compute/{org_id}/{user_id}`, read as lun's own vault identity)
  — the project contains no connection target, and the session's graph writes
  land in exactly that user's schema, as that user's role, or not at all;
- functions may declare `SecretStore`: the handler binds **per session** to
  the graph binding's prefix (`secret/data/graph/{org_id}/{graph_id}/`) and
  grants `getValue` on exactly the declared names — a secret is never an
  input, a node or an output, and the value type is opaque (no `ToJson`, no
  rendering), so it cannot appear in a node answer by accident;
- functions may declare `ObjectStore` for `storage` sinks: the handler relays
  each write to `liaison` (`POST /v0/egress`) with the attached warrant — lun
  never holds a storage credential, and the chokepoint holds where a
  credential is actually at stake.

## 4. State

`{workdir}/builds/{id}/` and `sessions/`. All reconstructible: builds re-derive
from the repository (same request, same id, once `LUN_ID_SALT` is set), session
state re-derives from the app's input log ([`../computations.md`](../computations.md)
§2 — this is why the app records every input it feeds). No credential is ever
written to disk.

## 5. Core types

A **function** is a function of the project under a declared signature
`α₁ → … → αₙ → Eff effs β` — every argument and the result a JSON value
(`FromJson`/`ToJson`), `effs` a row of vetted effects handled by linen's own
handlers: `Trace`, `Error`, `HTTP cap`, `FileSystem cap`, and — the additions
computations need — `PostgreSQL cap`, `SecretStore cap` and `ObjectStore cap`,
all linen's capability-restricted effects:

- **`PostgreSQL cap`** — the connection target (host, port, database, user)
  is a field of the **capability, not the code**: `Eff [PostgreSQL cap] α`
  cannot name another database or role. Queries are a structured AST with
  bound parameters and no `rawSql` escape hatch — injection and "checked SQL
  disagreeing with sent SQL" are both unrepresentable. At run time lun
  instantiates the capability from the session's user binding (§3).
- **`SecretStore cap`** — `getValue` and `describe` are separate permissions,
  names are segments so a prefix can be scoped, and the value is linen's
  opaque `Secret.Value`: no `ToJson`, no rendering — a program holding the
  value cannot print or serialise it by accident. Bound per session to the
  graph's declared names (§3).
- **`ObjectStore cap`** — the bucket and prefix are the capability's; the
  handler relays through `liaison` with the session's attached warrants (§3),
  so no storage credential ever exists inside lun.

A **graph** is a program in `Reactive`: named `input`s and applications of the
declared functions, nothing else — linen's other operators are refused, every
function in the graph is replaced by the declared function it names before it
runs.

## 6. What is proven

**Tier 1 · Type:**

- wiring a function to a value of the wrong type does not compile — graphs are
  typed by construction, acyclic by construction;
- a graph may only apply the declared functions — its definition is walked
  through every non-library constant it reaches; the graph builder's own
  primitives are refused;
- every effect in a function's row is one of the vetted ones, handled by
  linen's own `Handler _ IO` instance, **not one the project defines** — a
  project cannot smuggle in a handler that ignores its capability;
- a function without `PostgreSQL` in its row has no term that reaches a
  database; one without `SecretStore` no term that reads a stored secret; one
  without `ObjectStore` no term that writes to a bucket.

**Tier 3 · Theorem** (in linen, inherited): the capability's statement-kind and
table-scope obligations are proofs at the call site, discharged by `decide` at
elaboration time — a sink that tries the wrong statement or the wrong table does
not build.

## 7. What is not proven

The list that matters, none of it a proof:

- **The container is the boundary, not the types.** A user function with
  `HTTP` can reach any URL; with `PostgreSQL` only its own schema (the role
  enforces that), but `FileSystem` can read what the container holds, and
  untrusted projects still build with their own lakefile — linen's
  `System.GitFn.Policy` check-before-compile is the fix and is not yet how lun
  builds. Both are on lun's TODO.
- **SSRF:** `HTTP` must refuse non-`http(s)` schemes and the source scheduler
  must refuse private-range URLs ([`../computations.md`](../computations.md) §3);
  a test obligation (tier 5), not a type.
- **A session id is its capability** (32 random bytes) — nothing binds a
  session to a user beyond the app being the only one with the token. The user
  binding (§3) is data lun is told, not a fact lun verifies; the database role
  is what actually enforces it, which is why the credential is resolved from
  the binding server-side and never accepted from the caller.
- **One process per call** — no long-lived workers, so a graph request is one
  instant, not a stream; costs and latency scale with rebuilds, tier 6.

**Tier 5 · Test:** every refusal (bad signature, unvetted effect, own handler,
non-library constant in a graph, wrong arity) is tested in lun's e2e suite; the
new `PostgreSQL` refusals (wrong statement kind, wrong table, cross-schema)
belong beside them.

## 8. Open questions

- **`Time`** — a node that needs "now" (every source description that says
  "today's rate") has no vetted way to ask. Adding a `Time` effect means
  deciding what "now" means for a node that re-runs on every update.
- **Live pushes** — sessions are pulled today (the app's scheduler feeds
  inputs); a session that subscribes would need lun to keep workers alive, the
  thing one-process-per-call deliberately avoids.
- **Per-org instances** if build load or trust domains demand it; today one
  fleet-wide lun with a token the app holds.
