# Typednotes v0.10.0 (local implementation)

- Standard-page setup guidance; legacy `/onboarding` redirects, no wizard boxes.
- Atomic new-user default org/project/preferences via migration 0013; verified
  sign-in grants default welcome credits once. Existing defaults remain intact.
- Existing connection reauthorization/token repair with stable IDs, unchanged
  scopes/byte limits, closed old mints, pending failure state and revision fencing.
- Explicit repository-scoped notebook-write grant with no delete/org-policy widening.
- Durable actor-bound automatic generation on cell save, synchronized checkout,
  fresh bounded writer sessions and visible actual agent/file/check/publish activity.
- Shared caller-pin metadata for writer trials and final runtime builds; requires
  Lode v0.4.3. Lean LSP checks permit coherent unpinned parent type changes, while
  private checked manifests and Lun Output/Source/Wiring proofs retain user pins.

- Bounded loaded Lun workers with fresh request authority/context/logs, immutable
  graph templates, correlated frames, deadline retirement and no effect replay.

Publish Linen v1.11.0 first, then the dependent Lun v0.3.2. Lun's requirement and
immutable lock reference Linen's exact local release commit; normal locked Lean
builds were verified locally. Deploy it with Lode v0.4.3, app v0.10.0 and fleet
v0.6.3 (app SQL history through 0013). Service scopes and
shipped migrations are unchanged. The app
refuses a writer that does not acknowledge its build contracts. Release commits
and annotated tags are prepared locally; publication and cloud Apply remain the
user's actions.

Verified local API/browser/native writer/runtime and LSP fixtures are described
in [workspaces](workspaces.md), [native connectors](native-connectors.md) and
[Lun throughput](lun-throughput.md). Paid/live OAuth conformance remains distinct;
the signed-in live repository listing was also checked successfully, with the
read-only write ceiling separately confirmed. Production grants were not expanded.
