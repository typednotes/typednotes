# `lun` — the runtime

**Language:** Lean 4 + `linen` · **Repository:** [`typednotes/lun`](https://github.com/typednotes/lun)

**Current integration target:** Lun 0.4.1, Linen 1.12.0, Liaison's pure 0.6.0 SDK,
and the project-notes app with migrations 0014/0015. Publication/deployment remain
operator-owned. The sibling [Lun user guide](https://github.com/typednotes/lun/blob/main/docs/user-guide.md)
and [runtime guarantees](https://github.com/typednotes/lun/blob/main/docs/runtime-guarantees.md) are the wire/type references.

## 1. Purpose

Turn a Lean project pinned at a commit into **typed services**: named functions
under declared signatures, and typed graphs executed in bounded stateless steps.
The caller supplies previous state and input changes, receives every ordered changed
outcome, final nodes, new state and `nextCallAt`. Typednotes owns persistence,
serialization, scheduling and delivery; [`lode`](lode.md) implements the project notes.

**Not responsible for:** writing code (lode's, or the user's), credentials (a
private repository arrives through `liaison`, a sink's database credential is
read from the vault, never supplied by the caller), or deciding who may build
(its bearer token; the app holds it).

## 2. Dependencies

`linen` — `Control.Reactive` (graphs are its programs: acyclic by construction),
`Control.Monad.Effect` (the vetted rows below), `Database/SQL` (the
`PostgreSQL` handler), `Network/WebApp`, and Liaison's pure `Liaison.Wire` SDK.
The toolchain, `git`, Python temporary-file adapter, libpq and pre-built Linen
cache support generated drivers. Private repository fetches use native ancestry,
complete immutable tree and per-file views, never archives or signed downloads.

## 3. Interface

An HTTP service the app and lode call: `POST /v0/builds` (same request, same
build id), `POST /v0/builds/{id}/functions/{name}`, and
`POST /v0/builds/{id}/graphs/{name}`. There are no session routes. Graph calls take
`state`, `inputs`, fresh `binding`/`policy`/`connectors`, and optional millisecond
`now`; the app uses the runtime clock. `nextCallAt:null` waits for new input.

For computations, the contract additions ([`../computations.md`](../computations.md) §3–§4):

- initialization, input feeds and scheduled calls carry an authenticated **user
  binding** `(org_id, user_id)` and **graph binding** (`graph_id`), organization
  policy and fresh function-name operation/resource grants;
- a build request may declare functions whose rows include `PostgreSQL`; the
  handler is bound **per operation** to the user binding's credential (vault path
  `secret/data/compute/{org_id}/{user_id}`, read as lun's own vault identity)
  — the compiled capability contains non-secret host/port/database/role intent,
  which must match this resolved credential exactly. The graph's writes
  land in exactly that user's schema, as that user's role, or not at all;
- functions may declare `SecretStore`: the handler binds each operation to
  the graph binding's prefix (`secret/data/graph/{org_id}/{graph_id}/`) and
  operation-scoped declared names. Opaque `Secret.Value` has no `ToJson` or
  rendering, preventing incidental serialization. An authorized function can
  deliberately expose/use the value; this is not a noninterference proof;
- functions may declare `ObjectStore` for `storage` sinks: the handler relays
  each write to `liaison` (`POST /v0/egress`) with the attached warrant — lun
  never holds a storage credential, and the chokepoint holds where a
  credential is actually at stake.

The app's `server/local.rs` now provisions actor-bound compute and graph-vault
grants with independent connection/org/run documents and live declaration checks.
Execution bindings come from the authenticated app. Project grants and compiled
capabilities intersect fresh live ceilings on every request; state carries no authority.
The app sends `recoverInputs:true` on initialization
after adoption: historic incompatible JSON becomes an editable source error,
not a successful value supplied to functions.

## 4. State

`{workdir}/builds/{id}/` contains compiled artifacts. Warm workers retain code and
actor/entry-point affinity, not execution state or tokens. Typednotes stores state,
deadlines, revisions, input/outcome history and durable occurrences in PostgreSQL.
Lost responses can follow successful effects: retain an uncertain occurrence and
reconcile before an explicit restart; never automatically replay the previous state.

## 5. Core types

A **function** is a function of the project under a declared signature
`α₁ → … → αₙ → Eff effs β` — every argument and the result a JSON value
(`FromJson`/`ToJson`), `effs` a row of vetted effects interpreted by canonical
`Handler _ Execution` instances in `ReaderT ExecutionContext IO`: `Trace`,
`Error`, `HTTP cap`, `FileSystem cap`, `Connector cap`, `PostgreSQL cap`,
`SecretStore cap` and `ObjectStore cap`,
all linen's capability-restricted effects:

- **`PostgreSQL cap`** — the connection target (host, port, database, user)
  is fixed in the compiled capability; individual requests cannot substitute a
  connection. `BoundCompute` proves it matches the credential resolved only at
  `compute/{org}/{user}`. Queries are a structured AST with
  bound parameters and no `rawSql` escape hatch — injection and "checked SQL
  disagreeing with sent SQL" are both unrepresentable. At run time lun
  validates role/schema/target correspondence and consumes an `AuthorizedQuery`
  whose table schema matches that bound credential, with a 63-byte identifier limit.
- **`SecretStore cap`** — `getValue` and `describe` are separate permissions,
  names are segments so a prefix can be scoped, and the value is linen's
  opaque `Secret.Value`: no `ToJson`, no rendering — a program holding the
  value cannot print or serialise it by accident. Bound per operation to the
  graph's declared names (§3); read/describe/write/list have independent scopes.
- **`ObjectStore cap`** — the bucket and prefix are the capability's; the
  handler relays through `liaison` with the session's attached warrants (§3),
  so no storage credential ever exists inside lun.
- **`Connector cap`** — provider/connection, named operation and component-wise
  resource scopes; no caller-selected URL, authentication or raw HTTP fallback.
  Organization, connection, cell and warrant authority/byte ceilings intersect.

A **producer** declares `producer:true` and graph arguments followed by
`Nat → Option S → Eff effs (List B × S × Option Nat)`. `outputType` constrains B.
Linen's pure sequential helper supplies yield/yieldAll/wait scripts; effectful
producers use explicit typed steps. Typed continuations and pending bursts are
validated before execution. New argument events replace old continuations.

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
  canonical `Handler _ Execution` instance, **not one the project defines** — a
  project cannot smuggle in a handler that ignores its capability;
- a function without `PostgreSQL` in its row has no term that reaches a
  database; one without `SecretStore` no term that reads a stored secret; one
  without `ObjectStore` no term that writes to a bucket.

**Tier 3 · Theorem:** statement-kind/table/resource membership is carried in
effect types. `OutputContract`, `WiringContract` and `SourceContract` additionally
prove caller-owned result type, ordered named arguments and source presence/layout;
checked input constructors consume actual type equality. `BoundWiring` and
`BoundSources` consume equality with the actual graph at runtime. `ValidatedInput`
and `InputContract.decoder_sound` witness successful typed source decoding.

`AuthorizedRequest`/`NativeOperation` carry four-ceiling intersection and live
attenuation evidence; `BoundCompute`, `AuthorizedQuery`, `GraphSecret`,
`TemporaryPath` and `AuthorizedHTTP` bind executable targets. Narrowing,
scope confinement and authority/byte-bound properties are kernel checked. These
specific guarantees complement shape/closure audits; tests do not replace proofs.

## 7. What is not proven

The list that matters, none of it a proof:

- **Types do not replace build/container isolation.** Project executable closures
  and JSON dictionaries are transitively audited for unsafe/extern/implemented-by,
  axioms, initializers and raw IO/custom runners. Lakefiles/metaprograms still
  execute during compilation. Approved library/compiler/FFI/syscall behavior,
  PostgreSQL ACLs and server trigger/view semantics remain trusted.
- **Bounded local transports:** `HTTP` checks static scope, organization domains,
  standard ports and every DNS result; the typed transport pins a public numeric
  address with Host/TLS correspondence and follows no redirects. Temporary-file
  syscalls consume org/user-relative path witnesses and refuse unsafe symlink/
  hard-link reads. Socket/TLS/Python correspondence is a trusted boundary, not
  permission to reach arbitrary URLs or container files.
- **Authenticated caller trust.** The user
  binding (§3) is data lun is told, not a fact lun verifies; the database role
  is an additional enforcement layer. Private credentials resolve from the binding
  server-side. Local PostgreSQL/SecretStore authority trusts the authenticated
  app's minting and vault-protected projections; Lun does not independently verify
  their HMAC tags. Outbound Connector/ObjectStore warrants are HMAC-verified at
  Liaison. The app serializes state and chooses the actor; state cannot supply grants.
- **Warm-code cache:** bounded 1–16 workers with fresh context/Trace per request,
  actor-keyed leases and no failed-effect replay. Processes/pipes/mutexes remain
  trusted boundaries. The caller owns deadlines and external-effect idempotency.

**Tier 5 · Test:** Lun's unit suite and **70 compiled-runtime cases** pass locally,
including SCRAM queries, secrets, actual HMAC/SigV4 dispatch, local/live ceiling
denials, target substitution, source recovery and authority revocation. The app's
whole compiled writer/broker/local-Git/runtime positive/denial pipeline also passes;
supporting suites pass **108 API tests**, real project-note/browser/SQL scheduling
and outbox cases, and 15 app/broker groups plus 9 compiled app/runtime groups.
Provider responses are controlled peers; paid-provider/OAuth
conformance, real-model quality and Linux/container execution remain unmeasured
by these local checks.

Historical vault versions, binary object writes/custom write metadata and caller
pagination cursors remain explicit unsupported shapes. No raw IO/HTTP fallback
or unrestricted IO/HTTP fallback is implied by this release.

## 8. Open questions

- **`Time`** — a node that needs "now" (every source description that says
  "today's rate") has no vetted way to ask. Adding a `Time` effect means
  deciding what "now" means for a node that re-runs on every update.
- **Live pushes** — notification/subscription orchestration belongs to Typednotes;
  Lun emits bounded changes and deadlines without retaining executions.
- **Per-org instances** if build load or trust domains demand it; today one
  fleet-wide lun with a token the app holds.
