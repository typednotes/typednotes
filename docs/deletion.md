# Organization and account deletion

## Delete an organization

An owner opens **Organization settings → Danger zone** and types the org slug.
The API rechecks current ownership under the same org lock used for credential
provisioning and warrant minting. Members/admins cannot use the owner action.

Deletion first closes live connector gates and revokes tracked warrants, stops
writer/runtime sessions, ends public visitor sessions, deletes connection keys
and permission documents and notebook secrets, and drops the org's compute
schemas/roles and credentials. It then commits removal of the organization.
Database cascades remove its memberships, connections, projects, channels,
messages, notebooks, inputs/checks/shares and ledger usage/credits/holds.

Affected default preferences lose their removed org and all descendant pointers
and setup completion. A member with no organizations returns to `/onboarding`
with no default org/project/notebook. Other organizations and users stay intact;
valid existing workspaces can still be selected. The broker's text-only security
audit is intentionally retained, including denied/malformed attempts.

## Delete your account

**Account settings → Delete your account** requires your verified email address.
`POST /api/account/delete` deletes only the authenticated account; it does not
accept another user's identity as a deletion target. All sign-in identities,
browser sessions, memberships, connection keys and actor-owned compute resources
are removed. The current cookie is cleared; other browser sessions stop working.
Signing in again creates a fresh account, not the deleted user's old state.

The creator is a project's owner (`projects.created_by`). Projects owned by the
deleted user are removed with their notebooks, cells, channels and messages.
Other users' projects in surviving organizations remain. Notebook-level data in
those surviving projects belongs to the org/project, not a departing creator.
Public shares owned by the deleted user are revoked even on surviving projects.

An organization is removed when it has no remaining members. If other members
remain and you are its only owner, deletion refuses **before external cleanup**.
Open **Organization settings → Members** and use **Make owner** for an existing
member, then retry. No member is automatically promoted. Owners/admins cannot
demote/remove the last owner while other members remain, including concurrent
deletion attempts. Two co-owners can delete concurrently: checks serialize, and
the final empty organization is removed in the last transaction.

Billing history in a surviving organization remains, with the removed actor's
usage pointer set to null. Billing rows disappear with a deleted organization.
Preferences pointing at a removed project/notebook lose descendant pointers and
completion, while their surviving organization choice remains.

## Revocation, retries and boundaries

Older mint records identify credential owners, not every execution actor.
Account deletion therefore revokes all projections and resets active sessions
within affected orgs. Surviving notebooks re-register with fresh authority;
their definitions and data are preserved. Both external minting and compute
provisioning revalidate the executing membership **after** acquiring the org
lock, so an authenticated-but-queued request cannot mint for a removed member
through another person's surviving key.

The app/SQL/vault boundary is distributed. Failure retains transactional SQL
state for retry; already revoked authority, erased keys or dropped compute data
are not resurrected. Compute teardown is idempotent even when the role was
removed by a previous failed attempt. Runtime/vault outages do not report a
completed deletion. Already executing/fetched responses cannot be retracted.

Use the app/API for deletion: direct operator SQL does not perform vault/runtime
cleanup. Core ownership checks and referential actions trust the authenticated
Rust app and PostgreSQL locking/FK/trigger execution. Existing Lean broker/runtime
permission witnesses enforce the revoked effect ceilings; this is not a new
claim of a Lean proof of SQL cascades or distributed atomicity.

## Migrations and rollout

- App `migrations/0012_tenant_deletion.sql`: project ownership cascade, last-owner
  protection, empty-org/default cleanup and cross-history readiness check.
- Ledger `sql/0003_tenant_deletion.sql`: org FKs cascade; usage actor and ledger
  usage-event references use `SET NULL` where the parent survives elsewhere.
- Fleet source release **0.6.2** adopts app **0.9.1** and ledger
  **0.3.7** migration histories. Publish those new source tags and successful
  service images before the fleet's manual Plan/Apply. Existing tags and shipped
  SQL files remain unchanged. No new broker or runtime release is required.

If the ledger exists but still has restrictive FKs, destructive APIs return 503
before removing credentials, identifying the missing histories. An app-only
installation without ledger tables remains supported. Health requires readiness.

Local regression coverage uses real app HTTP/browser code, all app/ledger SQL
and a disposable PostgreSQL instance (including actual compute-role removal).
It verifies confirmations, denied roles, migration refusal, complete cascades,
onboarding resets, ownership transfer, failure/retry, retained other-user data,
session invalidation and concurrent account deletion. Native broker integration
also verifies queued shared-credential calls refuse a removed execution actor
before vault/provider reads; browser service peers are local mocks.
