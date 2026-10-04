# Scheduling ownership: Typednotes or Lun?

**Status:** Implemented locally against Lun 0.4.1 · **Reviewed:** 2026-10-04

## Question

For a task that repeats every hour, should Lun manage the schedule itself, or
should Typednotes call Lun every hour? Lun's evolving interface makes this a
timely architectural choice. How do Cowork and similar tools divide scheduling
from execution?

## Recommendation

**Typednotes owns durable scheduling; Lun owns execution and graph-internal
deadlines.** A Typednotes backend worker calls Lun when work is due. Lun executes
a bounded step and returns `state`, changes, and `nextCallAt`; Typednotes persists
the result and arranges the next call.

- **Typednotes:** user-visible schedules, timezone, pause/cancellation, ownership,
  build selection, persistent state, run history, dispatch, quotas, retries,
  missed-run/overlap policies, and notifications.
- **Lun:** typed function/graph execution, producer continuations, next execution
  deadlines, and effect-permission enforcement.

The worker can run separately from the web server. Scheduling does not require
a browser to remain open or a Lun worker to sleep between calls.

## Two meanings of “every hour”

- **Recurring task:** “Generate a report every hour.” Typednotes owns the cadence
  and creates each occurrence, potentially as an independent execution.
- **Continuous program:** “Check every hour until a condition holds.” A typed
  producer in Lun owns the recurrence logic and returns its continuation and
  deadline. Typednotes stores and resumes that execution. It should not duplicate
  the producer's scheduling rule.

Keep wall-clock cadence distinct from “one hour after completion”: slow runs and
downtime affect them differently.

## Rationale and comparable systems

This boundary keeps schedules durable across Lun redeployments, lets runtime
workers scale independently, and keeps lifecycle and authorization aligned with
Typednotes' authoritative product state. Temporal logic can still live in Lean.

Sources reviewed on the date above:

- **Claude Cowork:** saves task instructions and cadence; each occurrence runs
  as its own session. Documentation describes remote schedules, with transitional
  local-file exceptions. Anthropic's internal service boundary is not public.
  [Source](https://support.claude.com/en/articles/13854387-schedule-recurring-tasks-in-claude-cowork)
- **OpenClaw:** the persistent Gateway owns scheduling and recovery, with job
  definitions, runtime state, and history in shared SQLite. It must be running
  for schedules to fire.
  [Source](https://github.com/openclaw/openclaw/blob/main/docs/automation/cron-jobs/how-it-works.md)
- **OpenCowork (LeonGaoHaining/opencowork):** an Electron-side scheduler owns
  timers, a task store, and a queue; its executor calls the Agent Runtime API.
  [Scheduler](https://github.com/LeonGaoHaining/opencowork/blob/396da128b5eb0e69142e51cee57f936e53d1b89f/src/scheduler/scheduler.ts)
  · [Executor](https://github.com/LeonGaoHaining/opencowork/blob/396da128b5eb0e69142e51cee57f936e53d1b89f/src/scheduler/taskExecutor.ts)

The shared pattern is a persistent orchestration host driving execution.
Typednotes is the natural host for this architecture.

## Implementation implications

Lun 0.4.1 follows the caller-owned `state`/`nextCallAt` contract. Typednotes now
uses that graph-step API and requires `stateless-producers-v4` on ready builds.
Deployment requires the coordinated migration described below.

The previous scheduler advanced `next_due_at` before execution, allowing a crash
after claiming to lose an occurrence. The [current scheduler](../packages/api/src/server/scheduler.rs)
atomically advances the due marker and creates a stable durable occurrence. Its
implementation follows these requirements:

- Durable run records and stable occurrence identities.
- Per-execution serialization of timer wake-ups and input edits.
- Explicit missed-run and overlap policies.
- Fresh authorization on every call, including retries.
- Reconciliation of uncertain outcomes before replay: external effects may have
  succeeded even if a response was lost. A run ID alone does not provide
  exactly-once execution.

Reconsider ownership if Lun is intended to become a standalone **durable workflow
service**, responsible for execution storage, recovery, subscriptions, and run
history for multiple applications.

## Local implementation follow-up

The local implementation targets the documented Lun 0.4.1 API and append-only
migrations `0014_project_notes.sql` / `0015_durable_schedules.sql`. The local
fleet 0.7.0 declaration selects app source 0.11.0 through 0015. App/Lode/fleet
release commits/tags require publication before rollout; no deployment has occurred.

The implementation follows this ownership boundary:

- Typednotes persists graph state and `nextCallAt`, queues stable source/producer
  occurrence identities, and serializes graph steps against input edits.
- Source responses are persisted before delivery to the graph. Resuming a queued
  delivery does not repeat the source call.
- Returned state, outcome history and occurrence completion commit together.
- Every changed channel-sink emission is persisted in an ordered transactional
  delivery outbox. Delivery uses fresh live authority, never a replay of the graph.
  Lost send responses are uncertain and block later delivery rather than resending.
- Hourly source-check admission is serialized per organization and counts durable
  dispatches, including successful/uncertain checks and interrupted deliveries.
- Missing responses and expired dispatch leases become `uncertain`. Automatic
  replay is blocked; an explicit restart deliberately abandons the continuation
  after the caller reconciles possible prior effects.
- Current supported source cadence is UTC with overdue checks coalesced; graph
  steps serialize. Timezone selection, alternate missed-run policies, independent
  scheduler-worker packaging and schedule notifications are not implemented by
  this implementation.

Local SQL/browser tests exercise persisted queued work, obsolete-continuation
cancellation, lost responses, lease expiry and persisted source-response
delivery, ordered bursts and uncertain sends. Compiled app/Lode/broker/Lun fixtures
also pass against Lun 0.4.1, including producer declarations, emitted-value pins
and independent authority denials. See [release notes](release-0.11.0.md).
These tests do not establish exactly-once external effects. Database transactions,
service transports and Typednotes' orchestration remain trusted boundaries; Lun
continues to consume checked graph state and typed effect-permission evidence.
