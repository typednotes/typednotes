# Typednotes v0.9.1

Fixes organization deletion and adds confirmed self-account deletion, coordinated
with Ledger v0.3.7 and Typednotes-infra v0.6.2.

- Organization deletion cascades billing and workspace rows, revokes live grants,
  stops writer/runtime/public sessions, removes vault credentials and notebook
  secrets, and drops owned compute schemas/roles before committing SQL removal.
- Removed default targets clear descendant selections and setup completion.
  Org-less members return to onboarding without an org/project/notebook default.
- Account settings exposes email-confirmed deletion. Owned projects and their
  contents disappear; empty organizations are deleted. Other owners' projects
  and surviving organization billing remain. All account identities/sessions
  and private connections/compute resources are removed.
- A sole owner with other members must transfer ownership first. The Members UI
  can promote an existing member; SQL guards protect the last owner and concurrent
  deletion checks serialize. No automatic promotion is introduced.
- Credential minting and compute provisioning recheck the executing membership
  after the org lock. A queued request cannot use a surviving shared key after
  its caller is removed. External cleanup failures retain SQL state for retry;
  already-removed compute roles are safe to retry.

Append-only app migration **0012** supplies ownership/default lifecycle guards;
Ledger migration **0003** replaces restrictive billing FKs. Destructive APIs
refuse an older installed ledger before erasing credentials. The fleet adopts
both release histories before rolling out the new app. Existing migrations,
tags, broker/runtime wire formats and SDK pins stay unchanged.

Verified locally: **106 API tests**, **41 browser groups**, **13 real app/broker/SQL
groups**, **9 compiled app/runtime groups**, Ledger Lean tests, server/WASM builds,
offline fleet checks and shared workflow policy. Fixtures use disposable databases
and local provider/service peers; production data was not reset or deleted.
Core CRUD/SQL and distributed cleanup remain the documented trusted boundaries,
separate from the existing Lean effect-permission proofs.

Publish app v0.9.1 and Ledger v0.3.7 and wait for their exact-SHA main CI and image
publication, then publish the fleet v0.6.2 declaration and review its Plan before
manual Apply. Liaison v0.6.3 and Lun v0.3.1 remain required for the earlier repository
and public-share features. See [push order](push-order.md) and [deletion](deletion.md).
