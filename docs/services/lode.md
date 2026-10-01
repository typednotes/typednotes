# `lode` — the implementer agent

**Language:** Lean 4 + `linen` · **Repository:** [`typednotes/lode`](https://github.com/typednotes/lode)

**Coordinated release target:** Lode 0.3.0, Lun 0.3.0, Typednotes 0.6.0,
Linen 1.10.0, Liaison 0.6.0. Release files/pins/publication are parent-owned.

## 1. Purpose

Turn natural-language descriptions into Lean code: a graph's cells
([`../computations.md`](../computations.md)) become modules of typed functions
and the `lun.json` wiring them, written **into the project's own git repository**
on a branch shared with the user — open, `publish` (one commit, never a force
push), `lun_build`, fix, done. A `ui`, `endpoint` or `channel` source is an input
with a trigger, and a `secret` cell a declared capability: lode names them in
the graph and the `lun.json`, and never sees a secret *value*.

**Not responsible for:** holding any credential (it reaches the repository host
and the model through `liaison`, with warrants the app mints), deciding what it
may do (organization/session tool policy, agent selection, native authority and
warrants bound it; fuel additionally bounds its loop), or running what it wrote
(`lun` builds and runs it).

## 2. Dependencies

`linen` — `Network/WebApp`, `Network/HTTP/Client`, scoped filesystem/connector
types, `Data/Json`; Liaison's pure `Liaison.Wire` SDK. The Lean
toolchain and `lake` are on its `PATH`; the Docker image pre-builds a linen
package cache so a workspace does not rebuild linen.

**Gaps** (§8): no web access — the model cannot read a URL while implementing a
cell; and no LSP — its feedback loop is whole-`lake build` diagnostics, not
hover, goals or diagnostics at a point.

## 3. Interface

An HTTP service the app is the only caller of (`POST /v0/sessions`, messages,
long-poll the log, abort, refresh credentials; its README is the reference).
The app opens a session with the repository, branch, project path, the model,
and the organization tool list; cell descriptions arrive in the user message.
The app first obtains the actual persisted session ID, binds trusted conversation
and publication projections, then starts model → tools → model. Messages narrow
the live allowlist; credentials-only PUT refresh cannot change tool authority.
`check` is `lake build`; credentialed `publish` submits a broker-owned atomic
commit plan, not a generic provider call or git-push process; `lun_build` and `lun_call`
close the loop against `lun` itself — a session is done when the **published**
commit builds clean and answers as intended.

The app now attaches caller-owned `execution` at launch and re-mints the same
graph/cell grants on steering and credential refresh. `lun_call` remains
input-only; Lode privately merges the authenticated context after validating
launch/current narrowing and dispatch-time warrant caveats. Actor identity stays
separate from external credential ownership. Organization edits revoke outstanding
writer effects before acknowledgment, including anonymous HTTP and temporary
files. Persisted metadata holds public bounds only; restart drops every token.
See Lode's [`runtime-bridge.md`](https://github.com/typednotes/lode/blob/main/docs/runtime-bridge.md)
for the wire contract, consumed Lean witnesses, proofs and local real-runtime
fixture. Existing standalone sessions without a launch ceiling must be replaced
by a newly authenticated app launch to enable bounded trials.

The `plan` agent (read-only tools) proposes; `build` writes.

Model calls use named `inference.generate` through the native broker for Messages,
Chat, Responses, Gemini and Radius Pi/SSE. Typed `NativeContext` carries the actual
session ID and truthful user/agent initiator. The selected model fixes routing;
bounded inline local function tools/reasoning replay are permitted, while hosted
tools, remote retrieval and routing/header overrides are refused.

Repository checkout uses immutable native branch/tree/file reads and private
regular-file witnesses. Publication is confined to the notebook subtree and
expected branch head, with independent deletion permission. The broker uses
GitHub `updateRefs` head CAS and GitLab generated smart-HTTP receive-pack CAS;
there is no archive, signed-download or racy REST-commit fallback.

## 4. State

`{workdir}/sessions/{id}/` — the session record, the append-only `log.jsonl`,
the checkout. All of it is **reconstructible** ([`../computations.md`](../computations.md) §2):
the notebook re-derives it, the repository keeps what it published. Warrants are
kept **in memory only**, expire within minutes, and the caller sends fresh ones.
Metadata persists the launch `toolCeiling`, current `tools` and non-secret
credential bindings. Narrowing cannot be undone by a restart or credential refresh.

## 5. Core types

Tool arguments are parsed into typed values at the boundary where the model's
text becomes an action (`parse, don't validate`); the loop is **total by
construction** — `fuel` decreases structurally, so a run cannot diverge:

```lean
def run : (fuel : Nat) → … → IO Outcome
  | 0,          _ => pure .outOfFuel
  | .succ n,    … => …
```

The `lun.json` it writes is checked by lun against the code it ships: every
declared function **is** a function of its declared signature `A → Eff effs B`,
every effect in its row vetted, every graph built only from `input` and the
declared functions. The agent never gets to widen that contract — lun does the
checking, not lode's own judgement.

## 6. What is proven

**Tier 1 · Type:**

- tool arguments are parsed before dispatch; `AuthorizedArgs policy agent`
  carries permission for the operation derived from those exact arguments;
- `BoundedPolicy ceiling` retains evidence that the live tool list narrows the
  immutable launch list. The unchecked dispatcher is private;
- native checkout consumes private selector/bookkeeping/regular-file witnesses;
  lexical/path/symlink checks supplement those types at the filesystem boundary;
- `RuntimeInput` can emit only function `input`/`inputs` or graph `inputs`.
  The model cannot inject runtime policy, bindings, grants or credentials.

**Tier 3 · Theorem:** `run` terminates — structural `fuel`, no proof obligation
needed. `BoundedPolicy.authority_bounded` and `AuthorizedArgs.authority_bounded`
prove actual named tool authority is bounded; narrowing is reflexive/transitive.
The policy mutex serializes dispatch with narrowing, and metadata serialization
prevents an acknowledged narrower list from being overwritten on restart.

## 7. What is not proven

**Nothing about the code lode writes.** Not that it implements the cell's
description faithfully, not that it is efficient, not that it is resistant to
adversarial content the model read. The mitigations are structural, and they
are the whole design:

- authority intersects organization, connection, cell and warrant scopes at the
  broker, plus organization/session/agent tool lists at Lode dispatch; trusted run
  refinements bind conversation tools and the publication branch/subtree;
- credentialed publication consumes exact-head atomic native conditions; stale
  heads/concurrent rewinds are refused and write does not imply deletion;
- what it writes is **reviewable** — it is a commit in the user's own
  repository, diffable like any code;
- what it ships is **checked** — `lake build` clean, and lun's signature and
  graph checks (the same gates the user's own code would face);
- spend is bounded by the warrant's budget and the org's credits; every model
  call is metered by liaison and audited, attributed to the session.

**Tier 5 · Test:** that a steered/aborted run actually stops before the next
model call; that credentials sent as `LODE_MODEL_API_KEY` (development) never
reach the log.

Allowed `bash` can make changes without the named `write` tool, and `check` executes
the project's Lake configuration. Named-tool permission proofs are not a semantic
read-only shell or process-sandbox proof. Build/container/trust-domain isolation,
approved libraries, filesystem/zlib/socket/TLS FFI, broker HMAC/ledger and remote
Git/API correspondence remain trusted boundaries.

Local release verification passes the actual app → compiled Lode → real broker
→ local Git → compiled Lun positive and denial pipeline. Supporting suites pass
101 app API tests, 24 browser groups, 655 real broker HTTP cases and 69 compiled
runtime cases. Models/provider APIs in those tests are controlled local peers;
paid-provider/OAuth conformance is not measured by them.

**Tier 6 · Measure:** how reliably a real paid model drives a session to a green
lun build remains unmeasured;
steps and fuel exhausted per session.

## 8. Open questions

- **`fetch` tool** — let the model read a URL (docs, examples) while
  implementing a cell. It is a new egress: should it go through `liaison` (an
  anonymous-fetch call kind, [`../computations.md`](../computations.md) §8) or
  out of the container directly, and what rate applies?
- **Lean LSP is implemented** — bounded ephemeral `lake serve` workers provide
  hover, goals, completion, definition and diagnostics under the `lsp` permission.
  They do not require a long-lived process per session. See
  [`../graph-and-lode.md`](../graph-and-lode.md) for the closed questions and evidence.
- **Fuel vs budget** are two caps; deriving fuel from remaining budget would
  make one knob, at the price of coupling authority and spend.
- **Trust-domain deployment.** Its `bash` tool runs inside the container; path
  checks do not isolate allowed processes from other sessions. Separate org/trust
  domains require process/container isolation, not merely credential-less service
  configuration. Linux/container execution is separate from the local macOS checks.
