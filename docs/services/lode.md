# `lode` — the implementer agent

**Language:** Lean 4 + `linen` · **Repository:** [`typednotes/lode`](https://github.com/typednotes/lode)

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
may do (the warrants and its fuel bound it), or running what it wrote
(`lun` builds and runs it).

## 2. Dependencies

`linen` — `Network/WebApp`, `Network/HTTP/Client`, `FileSystem` capabilities
(paths never leave the checkout), `Data/Json`, `Liaison.Wire`. The Lean
toolchain and `lake` are on its `PATH`; the Docker image pre-builds a linen
package cache so a workspace does not rebuild linen.

**Gaps** (§8): no web access — the model cannot read a URL while implementing a
cell; and no LSP — its feedback loop is whole-`lake build` diagnostics, not
hover, goals or diagnostics at a point.

## 3. Interface

An HTTP service the app is the only caller of (`POST /v0/sessions`, messages,
long-poll the log, abort, refresh credentials; its README is the reference).
The app opens a session with the repository, branch, project path, the model,
and the cell list; lode's loop is then model → tools → model until the run
answers. `check` is `lake build`; `publish` pushes; `lun_build` and `lun_call`
close the loop against `lun` itself — a session is done when the **published**
commit builds clean and answers as intended.

The `plan` agent (read-only tools) proposes; `build` writes.

## 4. State

`{workdir}/sessions/{id}/` — the session record, the append-only `log.jsonl`,
the checkout. All of it is **reconstructible** ([`../computations.md`](../computations.md) §2):
the notebook re-derives it, the repository keeps what it published. Warrants are
kept **in memory only**, expire within minutes, and the caller sends fresh ones.

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

- paths cannot leave the checkout — linen's `FileSystem` capability, symbolic
  links resolved (a `..`-escape in linen's scope check is a known open issue,
  [`lun`'s TODO](https://github.com/typednotes/lun) — the container is the
  boundary until it is fixed);
- a tool call is constructible only from parsed, validated arguments.

**Tier 3 · Theorem:** `run` terminates — structural `fuel`, no proof obligation
needed.

## 7. What is not proven

**Nothing about the code lode writes.** Not that it implements the cell's
description faithfully, not that it is efficient, not that it is resistant to
adversarial content the model read. The mitigations are structural, and they
are the whole design:

- authority is only ever what the warrants carry — repo `write` on **one
  branch**, the model with a per-call budget; nothing ambient, nothing
  retained;
- `publish` never force-pushes, so it cannot overwrite someone else's work;
- what it writes is **reviewable** — it is a commit in the user's own
  repository, diffable like any code;
- what it ships is **checked** — `lake build` clean, and lun's signature and
  graph checks (the same gates the user's own code would face);
- spend is bounded by the warrant's budget and the org's credits; every model
  call is metered by liaison and audited, attributed to the session.

**Tier 5 · Test:** that a steered/aborted run actually stops before the next
model call; that credentials sent as `LODE_MODEL_API_KEY` (development) never
reach the log.

**Tier 6 · Measure:** how reliably a real model drives a session to a green
lun build — unmeasured today (lode has never been run against a real model);
steps and fuel exhausted per session.

## 8. Open questions

- **`fetch` tool** — let the model read a URL (docs, examples) while
  implementing a cell. It is a new egress: should it go through `liaison` (an
  anonymous-fetch call kind, [`../computations.md`](../computations.md) §8) or
  out of the container directly, and what rate applies?
- **Lean LSP loop** — `lake build` diagnostics say *that* something is wrong,
  not *where the model should look*; an LSP session (hover, goals, diagnostics
  at a point) would close the gap between lode's loop and the user's editor.
  Cost: one more long-lived process per session.
- **Fuel vs budget** are two caps; deriving fuel from remaining budget would
  make one knob, at the price of coupling authority and spend.
- **One lode per org?** Its `bash` tool runs what the model asks inside the
  container, so the container is the isolation boundary. One fleet-wide,
  credential-less lode is acceptable at v1 ([`../computations.md`](../computations.md) §6);
  per-org instances are the honest end state if graphs get adversarial.
