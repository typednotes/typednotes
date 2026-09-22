# `agent` — planning and reasoning

**Language:** Lean 4 + `linen`

## 1. Purpose

Decide what to do on the user's behalf, and do it by calling [`broker`](broker.md). Uses
inference from Baseten, Mistral or Scaleway.

**Not responsible for:** holding credentials, reaching third parties directly, or deciding
what it is allowed to do. It holds a warrant and nothing else.

## 2. The honest framing

This is the one service where the interesting property is **containment, not correctness.**

The agent's behaviour depends on a model's output, so no theorem can say what it will
decide, and none can say the decision was good. What *can* be established is that a bad
decision — including one produced by an adversarial instruction inside a user's own
document — cannot exceed the authority and budget the run was given.

Containment is not implemented here. It is inherited from types `broker` and `core` define:
`Reserved r` is unobtainable without a verified warrant, and `attenuate` only narrows. This
service's job is to not undermine that.

## 3. Dependencies

`Network/HTTP/Client` for inference calls (or delegate routing to `broker` — see its §8),
`Crypto/JOSE/FFI.hmac` to attenuate warrants, `Data/Json`, `Control/Concurrent` for
parallel tool calls.

## 4. Core types

Attenuate before every sub-call, so each step runs with strictly less authority:

```lean
/-- Drop every capability except those listed, and halve the remaining budget. -/
def narrow (w : Warrant) (keep : List (Provider × Action)) (budget : Credits)
    : IO Warrant
```

The loop is **total by construction** — fuel decreases structurally, so it cannot diverge:

```lean
def run : (fuel : Nat) → Plan → Warrant → IO Outcome
  | 0,          _, _ => pure .budgetExhausted
  | .succ n, plan, w => do
      let step ← decide plan w           -- inference; nondeterministic
      match step with
      | .done o      => pure o
      | .call req    => do
          let w' ← narrow w [req.capability] (w.remainingBudget / 2)
          let res ← broker.invoke w' req
          run n (plan.extend res) w
```

Tool arguments are parsed into validated types before the call, never passed as raw model
output — `parse, don't validate` at the boundary where the model's text becomes an action.

## 5. What is proven

**Tier 1 · Type:**

- the agent cannot reach a provider except through `broker.invoke`, because it has no HTTP
  client for third parties and no path to [`secrets`](secrets.md);
- `run` terminates — `fuel` is structurally decreasing, so no proof obligation is needed;
- a tool call is constructible only from parsed, validated arguments.

**Tier 3 · Theorem:**

```lean
/-- Authority never grows across a run, at any depth of sub-call. -/
theorem run_authority_bounded (w : Warrant) (fuel : Nat) (plan : Plan) :
    ∀ req ∈ (run fuel plan w).calls, w.permits req
```

This follows from `attenuate_monotone` by induction on `fuel`, and is the only theorem this
service really owes. Combined with `core`'s `mint_sound`, the chain is complete: a warrant
starts no wider than the user's authority, and narrows at every step.

## 6. What is not proven

**Nothing about decision quality.** Not that the plan is sensible, not that the agent
interpreted the request correctly, not that it is resistant to prompt injection. The
mitigation is entirely structural:

- authority bounded by the warrant, and narrowing at every step;
- spend bounded by the per-run cap;
- every attempt in the audit log, attributed to a `run_id`;
- side-effecting calls gated on human confirmation in `broker`.

**Design consequence, and it matters more than any theorem here:** warrants must be minted
**narrowly per run**, from the user's actual request. A convenience warrant covering
everything the user can do discards the whole protection while leaving every proof above
still true.

**Tier 5 · Test:** that `narrow` is actually called on every sub-call path — the theorem
covers `run` as written, but a future code path that forwards `w` unchanged would type-check
fine. This is the service's main regression risk.

**Tier 6 · Measure:** inference cost per run against the budget model in
[`ledger`](ledger.md); how often runs exhaust fuel (a sign the cap is wrong, not that the
agent is looping).

## 7. Open questions

- **Fuel as a proxy for budget.** Currently two separate caps — structural `fuel` and
  `Credits`. Should fuel just be derived from remaining budget?
- **Halving the budget per sub-call** is arbitrary and will starve deep plans. Needs a real
  allocation policy.
- **Where does inference routing live** — here or `broker`? See `broker` §8; cost accounting
  argues for `broker`.
- **Does `linen` need a `Lean`-side inference client**, or does every provider call go
  through `broker`? The latter is better for metering and is the current assumption.
