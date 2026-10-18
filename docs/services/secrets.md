# `secrets` — vault

**Language:** Rust · **Repository:** https://github.com/typednotes/secrets · **Status:** built

For the current clients, path ownership and trust boundaries, see
[the implemented architecture](../system-architecture.md). The Lean client/type
sketches retained below are design material, not proofs of the Rust vault.

## 1. Purpose

Authenticate clients, enforce path/capability policies and store encrypted secret
values. Typednotes uses it for provider credentials, independent live policy/run
projections, graph secrets and actor-bound compute credentials. The server also
has additional engines; their existence does not imply all are wired into the app.

Secrets enforces its own client authentication and ACLs. Product membership and
ownership checks belong to the app; delegated warrant/operation authorization
belongs to [`broker`](broker.md). These are independent ceilings.

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

The app writes/deletes provider credentials and publishes trusted policy documents
with a write-only credential identity. Liaison reads/refreshes provider credentials
behind its authorization path. Lun resolves bounded local compute/graph-secret
credentials with its own service identity. The browser and model receive no raw
vault identity or stored provider-token readback; generated functions use checked
effect handlers rather than an unrestricted vault client.

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
