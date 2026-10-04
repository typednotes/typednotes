# Typednotes v0.11.0 — project notes and stateless Lun integration

This locally verified source release is coordinated with Lode v0.5.0 and fleet
v0.7.0. Commits and annotated release tags are local; publication and deployment
remain pending. Existing release tags and shipped SQL histories remain immutable.

## Product workflow

- **Org → Project → Notes.** Notes live directly in a project; one project model
  and operation/resource permission set replace notebook/per-note setup layers.
- Repository/AI-first setup derives organization/project names from verified
  repository metadata, supports persistent skip/manual setup and idempotent retries.
- Repository and model controls have responsive aligned layouts. Model inventory
  loads automatically; catalog failure offers a retry rather than routine refresh.
- Automatic generation returns a writer session before checkout, uses durable
  revision/message keys and bounded startup retries, and shows actual activity.
  Bounded user questions pause work; only the initiating actor answers after live
  authority refresh. A queued checkout intent cannot be mistaken for completed work.

## Stable execution contract

The runner is **Lun 0.4.1**, with **Linen 1.12.0** and Liaison's pure **0.6.0 SDK**.
Ready builds must attest `stateless-producers-v4`; older artifacts fail closed.

Typednotes owns private graph state, revisions, deadlines, UTC cron cadence,
serialization, durable occurrence history and ordered delivery outboxes. Calls use
fresh policy/binding/grants. `producer:true` and output contracts survive app
manifest forwarding; output pins constrain each emitted value. The initial call,
input edits and timer wake-ups all persist the returned state. Source fetch results
are persisted before graph delivery, so recovery does not repeat the fetch.

Missing responses/expired dispatch leases become uncertain. Automatic replay is
blocked, and an explicit reviewed restart abandons old continuations/deliveries.
This does not provide exactly-once external effects. Scheduling transactions,
service transports and app-to-protected-projection correspondence remain trusted
boundaries; Lean witnesses/proofs continue to govern runtime state/typed inputs,
output/wiring equality, static effects and operation/resource authority.

Public visitors have separate app-owned state. Every public request supplies only
Trace/Error, empty domains and no connector grants. Public tokens expose neither
runtime state nor actor credentials; they cannot supply an execution envelope.

## Append-only migration

**0014** merges existing notebooks into one canonical project graph. It preserves
note IDs, output pins, input/outcome history and original config/implementation
records; qualifies duplicate note/input names and rewrites their references.
Secret values retain physical names/namespaces via protected same-project routes.
Other graphs are archived. Implementations require regeneration; old public links
are revoked and project connection/local-service grants start empty for review.
Old writer/runtime handles remain in a durable retirement queue until all external
authority retirement steps acknowledge success.

**0015** adds state revisions, durable source/producer occurrences, source-result
handoff, quota accounting and ordered channel-delivery outboxes. New serving code
requires these tables; it does not migrate schemas at startup.

The oldest canonical graph's repository folder/model selection is retained. Other
historical graph metadata remains available for recovery; choose the unified model
in project settings if the previous notebooks used different models.

## Publication and rollout

Release versions: app **0.11.0**, Lode **0.5.0**, fleet **0.7.0**. Lode's image,
dependency lock and new-project default all select Linen **1.12.0**. Its older
Lun dependency is a pure execution-projection/attenuation proof SDK, not use of the
removed runtime session API. Broker operation catalog and ledger SQL are unchanged.

1. Publish the existing local Linen 1.12.0 and Lun 0.4.1 release refs with successful
   exact-commit main CI. Publish the matching local app/Lode release commits/tags.
2. Drain old writers/runtime requests and pause old application dispatch/input
   traffic for the breaking session→state transition. In-flight effects must be
   reconciled before proceeding; SQL alone cannot revoke external authority.
3. Publish source/images before the fleet's reviewed Plan/Apply. Its app history
   reads the real service SQL at 0.11.0 through 0015; images resolve `latest` to
   immutable digests. Roll out the matching app, writer and stateless runner as
   one coordinated change, then verify retirement-queue completion.
4. Review project grants/model selection, regenerate code, validate source/sink
   behavior, then resume dispatch and publish new public links as needed.

Cloud Plan/Apply, production credentials and production data were not accessed
during this work. Linux/container and live paid-provider/OAuth checks remain CI/
operator verification rather than local claims.

## Local verification

- 108 API tests; server build/check and WASM check.
- Real Dioxus/Playwright/PostgreSQL project-note/setup/layout/question cases.
- Populated-data note merge preserving references, history, pins and secret origins.
- Durable queued work, stale continuation cancellation, source-response handoff,
  lost responses, expired leases, ordered burst delivery and uncertain-send refusal.
- 15 app/broker/SQL groups, 9 actual compiled app/runtime groups, and the full
  app → compiled Lode → HMAC broker → local Git → compiled Lun producer pipeline.
- Lun/Lode unit suites; 70 real compiled runtime cases with SCRAM, vault effects,
  independent ceilings, typed sources and fresh authority.
- Real slow Git checkout exceeding 30 seconds, question pause/resume/cancellation,
  immutable retry identity and duplicate-intent suppression; warning-free local
  Lun/Lode builds with the fixed Linen TLS implementation.
