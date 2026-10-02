# Typednotes v0.9.0

## Workspace UX

- Repository browsing groups accessible repositories by personal account or
  organization. Pages contain at most 100 repositories; load more up to page 100,
  or enter `owner/repo`/a GitHub or GitLab HTTPS repository URL. Save verifies
  exact resource metadata rather than requiring presence in the first page.
- Pickers are controlled, reset correctly and label their actual trigger
  buttons. Long menus scroll. Changing accounts cannot retain another account's
  repository/channel choice; local filtering/text entry issue no per-character
  inventory queries.
- Connected providers lose their creation widgets. API and database insertions
  enforce one connection per provider/organization; historical rows and credential
  references remain intact. Existing messaging identities can be reused.
- Default organization/project/notebook preferences drive the sign-in landing.
  First-time setup is resumable and checks required repository/model/tool grants
  before completion. Optional integrations remain skippable. Inline text,
  tooltips and keyboard-accessible explanations define the workspace hierarchy.
- Notebook essentials stay visible; model/cost settings, writing notes, view
  options, implementation/wiring, activity and maintenance unfold on demand.

See [the workspace guide](workspaces.md) for details and setup instructions.

## Public read-only sharing

Notebook creators and org admins can publish an immutable snapshot with an
unguessable, hash-stored link and revoke it later. Anyone with the link sees its
descriptions, non-secret input values and results. Secret cells and their runtime
nodes, credential bindings, code and settings are excluded from every public
response. Results can contain data from private sources; the publishing control
makes their publication explicit. Only declared UI inputs can be submitted.

Each browser receives an isolated 30-minute visitor session; edits do not write
the author's graph input log or runtime session. Execution grants no connectors,
credentials, domains or external effects. Lun's private `PureShareExecution`
witness proves that effects are only Trace/Error and consumes the evidence on
session start and update. Database/storage writes, messages, paid model calls,
secrets and other external effects are refused before their IO. Effectful nodes
show a denied outcome in public recalculation.

Limits are 1 MiB per input, 60 input attempts/minute per share (including runtime
refusals) and 200 active visitors per share. Expired sessions/call records are
cleaned by the app scheduler. Revocation removes stored visitor sessions and
ends their associated runtime sessions. Already fetched responses cannot be
retracted. Build loss requires the author to rebuild and renew the snapshot;
there is no credentialed public rebuild fallback.

## Contracts and rollout

Publish **Liaison v0.6.3**, **Lun v0.3.1** and **Typednotes v0.9.0**, then deploy
**Typednotes-infra v0.6.1**. Its migration history adopts append-only 0009
(provider guard), 0010 (user workspaces), 0011 (shares/visitors) before app rollout.
Previously shipped migrations and dependency source pins are unchanged. No
production data reset was required.

The user may push each branch and its new tag together; wait for main CI and image
publication before the fleet's manual Plan/Apply. Liaison 0.6.3 is required for
page-text payloads and repository metadata mode; there is no raw HTTP fallback.
Lun 0.3.1 validates and persists the pure-share marker. See [push order](push-order.md).

Local checks passed: **106 API tests**, **33 browser groups**, **675 real broker
HTTP cases** with zero catalog drift, **12 app/broker/SQL groups**, **9 compiled
app/runtime groups**, broker/runtime Lean suites and offline fleet checks. Fixtures use isolated
PostgreSQL and local provider peers. OAuth/paid-provider conformance, SQL
isolation/durability, native FFI and build/process isolation remain the documented
trusted boundaries; tests supplement, rather than replace, the Lean proofs.
