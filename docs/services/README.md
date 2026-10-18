# Services and component contracts

Start with the [implemented system architecture](../system-architecture.md) for
current roles, SVG diagrams, state ownership, execution/authority flows and
deployment packaging. [Architecture decisions](../architecture.md) is a historical
design draft. [Proof strategy](../proof-strategy.md) defines the shared vocabulary;
actual service code and current guarantee documents determine which claims hold.

## Implemented component map

These are logical boundaries, not a requirement to run one container per row.
Full, Lite and Local package them differently; see the overview's deployment section.

| Component | Implementation | Precise role |
|---|---|---|
| [App / core](core.md) | Rust fullstack API | Product identity/ownership, projects/notes, policy, warrant minting, durable state, schedules and delivery orchestration |
| [Web](web.md) | Dioxus UI in the app | User interaction and rendering; no stored credential readback or independent execution authority |
| [Lode](lode.md) | Lean service | Bounded code-writing sessions, compiler/LSP feedback, scoped publication and runtime trials |
| [Lun](lun.md) | Lean service | Check/compile declared functions and graphs; execute bounded stateless steps with fresh authority |
| [Broker / Liaison](broker.md) | Lean service with native boundaries | Credentialed connector authorization, reservation, transport, audit and settlement; native paired messaging |
| [Secrets](secrets.md) | Rust service | Authenticated path-policy secret storage, encrypted barrier and engine composition |
| [Ledger](ledger.md) | Lean library/service | Credit/hold/usage schema and model; serving process sweeps expired holds; request-path consumers use shared SQL |
| [Linen](https://github.com/typednotes/linen) | Linked Lean library | Effects, capabilities, graphs, proof-carrying values and service/FFI primitives |
| [Infra](https://github.com/typednotes/infra) | Lean deployment engine | Typed desired/observed infrastructure, provider adapters, plan/apply |
| [Typednotes Infra](https://github.com/typednotes/typednotes-infra) | Fleet and packaging project | Concrete resources, access, image assembly, migration orchestration and combined-image supervision |

`core` and `web` are currently roles of the same Rust application. The scheduler
is app background work, not an independently packaged service. Ledger is an
actual sweeper process plus a shared SQL contract, not just a module deployed
inside core. Linen is not an HTTP service.

## Proposed or abstract roles

- [`idp`](idp.md) describes a proposed Typednotes OIDC/passkey service. Current
  human login uses the app's external provider flows and its own sessions.
- [`agent`](agent.md) describes a generic planning/reasoning role. Lode is the
  implemented coding agent; there is no additional generic-agent service required
  by the current app → Lode → Lun path.
- The standalone Lean core and some guarantees/interfaces in the older service
  sketches are design proposals. Their presence is not evidence of implementation.

## Where exact contracts live

- [Native connector integration](../native-connectors.md),
  [connections](../connections.md) and [paired messaging](../paired-messaging.md).
- [Scheduling ownership and recovery](../scheduling-ownership.md).
- [Lun runtime guarantees](https://github.com/typednotes/lun/blob/main/docs/runtime-guarantees.md)
  and [Lode runtime bridge](https://github.com/typednotes/lode/blob/main/docs/runtime-bridge.md).
- [Liaison native permissions](https://github.com/typednotes/liaison/blob/main/docs/connector-permissions.md)
  and [native paired adapters](https://github.com/typednotes/liaison/blob/main/native/README.md).
- [Fleet declaration and deployment variants](https://github.com/typednotes/typednotes-infra/blob/main/README.md).

## Service-document checklist

Each detailed service document should state:

1. Purpose, current implementation status and source owner.
2. Dependencies and real deployment/process boundaries.
3. Consumed/provided APIs and their authentication/authority contracts.
4. Durable data, transient state and recovery ownership.
5. Core types and which execution paths consume their evidence.
6. The exact properties proved, with code references.
7. Tests, unverified axes and remaining trusted boundaries.
8. Planned work, clearly separated from implemented behavior.

Keep these contracts, the overview and SVG diagrams synchronized with code.
Section 7 is essential: a service document listing only proofs is misleading
about the operational and native trust boundaries.
