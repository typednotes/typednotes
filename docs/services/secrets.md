# `secrets` — vault

**Language:** Rust · **Repository:** https://github.com/typednotes/secrets · **Status:** built

## 1. Purpose

Store and serve third-party credentials — the OAuth refresh tokens behind "connect to many
services". Modelled after OpenBao. Scoped per `(user_id, provider, connection_id)`.

**Not responsible for:** deciding who may fetch a credential (that is a warrant check in
[`broker`](broker.md)).

## 2. Why this one stays in Rust

Not inertia. Two principled reasons:

1. **Constant-time operations are its core job.** Sealing, unsealing, key comparison and
   policy evaluation on secret material all need timing independence. Lean cannot express
   that property, and its reference-counting runtime works against it.
   `../proof-strategy.md` lists constant-time as never-provable in Lean; a vault is the one
   service where that limitation is central rather than incidental.
2. **It is shipped and working.** A rewrite would trade a working component for risk in the
   most security-sensitive service in the system.

The Lean-4 reasoning in `../architecture.md` §3.1 — provable pure core, shared trace layer —
does not transfer here. A vault's hot path is crypto and I/O, which is exactly the part no
proof reaches.

## 3. Interface

Consumed only by [`broker`](broker.md), and only after a warrant check. Never by
[`agent`](agent.md), never by [`web`](web.md), and a credential is never returned to a
client.

A `linen` client follows `Cloud/Secret/{SecretsManager,ScalewaySecretManager}` as templates.
The client type should carry the scope so a call site cannot request a credential outside
the triple it holds:

```lean
structure ScopedRef where
  private mk ::
  userId       : UserId
  provider     : Provider
  connectionId : ConnectionId

/-- The only constructor: derived from an authorized request, so scope cannot be widened. -/
def ScopedRef.ofAuthorized (a : Authorized r) : Option ScopedRef
```

That is a tier-1 guard on the *client* side. It says nothing about the server, which
enforces its own policy independently — and should, since a client-side type is not an
access control.

## 4. What is proven

Nothing, in Lean. This service is outside the proof story by design.

Its own guarantees are conventional: Rust's memory safety, its policy engine, its audit
log, and whatever test and review coverage the repository carries. Assess it on those terms
rather than by comparison with the Lean services.

## 5. What is not proven — and matters

- **Availability.** `broker` depends on `secrets` for every egress call. Failure behaviour
  must be **fail-closed**. Whether any caching is acceptable is undecided (§6) and is a real
  security/latency trade, not an optimisation.
- **Scope enforcement server-side.** The `ScopedRef` type above is a client convenience. The
  server must enforce the same scope independently; if it does not, the type is decoration.
  Verify this before relying on it.
- **Rotation.** Refresh-token rotation by upstream providers means stored credentials change
  under us. The write path needs to be idempotent and last-write-wins per connection.

## 6. Open questions

- **Fail-closed with what caching, if any?** A short-lived in-`broker` cache of *access*
  tokens (not refresh tokens) is the obvious middle ground, and needs a decision.
- **Does the server enforce the `(user, provider, connection)` scope**, or only
  authenticate the caller? *Answer (read in 1.2.0):* only by path prefix. Policies are
  longest-prefix rules over paths, so the scope holds only because the triple is in the path
  (`secret/data/thirdparty/{provider}/{user_id}/{connection_id}`, see
  [`../connections.md`](../connections.md)). The per-connection binding is enforced in
  `broker`: the account's last segment must equal the warrant's `resource`.
- Is the audit log in `secrets` reconciled against `broker`'s? Two logs that can disagree
  are worse than one.
