# App/native connector integration

## Repository selection contract (Liaison 0.6.3)

`repositories.list` uses resource `[]` and optional canonical decimal-text `page`
1–100. Broker-owned transports use fixed 100-item pages and deterministic provider
ordering; both broker and app reject inventories exceeding 100 entries. Namespace
grouping derives from those repositories; no organization-directory grant is added.
`repositories.read` metadata uses exact resource `[owner,repo]` and payload
`{"view":"metadata"}`. The broker's private `Metadata` witness matches returned
identity to the authorized resource; the app additionally validates the canonical
GitHub/GitLab URL. Direct selection does not need account-wide listing authority.

Operation IDs, resource scope rules, permission presets, OAuth ceilings and the
four independent authority bounds are unchanged. The permission UI explains the
bounded account inventory. Updated parsers, native plans, replies and denial tests
are coordinated with the app. Older broker payload refusal has no raw IO fallback.

Migration 0009 additionally guards one provider connection per organization,
under the same transaction lock as provisioning; health requires that enabled
guard and the 0010/0011 workspace/share tables. See [workspaces](workspaces.md).

Public viewer sessions use Lun 0.3.1's private `PureShareExecution` witness with
Trace/Error-only effects, no domains and no connector grants. Every session start
and update consumes checked evidence; no broker warrant is minted. Effectful
graph nodes refuse before credential/local DB/storage IO. Public snapshot filtering
is an app responsibility, tested separately from the proved effect ceiling.

## Trusted provisioning

Tenant deletion and credential provisioning share the org transaction lock.
Minting now carries the actual executing user separately from credential ownership
and rechecks that user's live membership after the lock. Account deletion closes
all affected org gates/projections and resets active sessions before removing
credentials/rows. See [deletion and its trusted boundaries](deletion.md).

The app sends `kind: connector`, a named operation, component-wise resource list
and JSON payload string. It supplies no provider URL, HTTP method, authentication
header or raw fallback. Warrant time/credit fields remain decimal strings.

`packages/api/src/server/connector.rs` reloads connection identity, state, permissions and
validated organization policy under a per-org PostgreSQL transaction advisory
lock. Requested authority must narrow the connection; organization policy
intersects the cell. Unknown operations/fields, malformed policies/selectors and
out-of-range byte limits are refusals. Before releasing a warrant it provisions:

- `thirdparty/{provider}/{credential-owner}/{connection}/permissions`: connection ceiling.
- `connector-policy/{org}/{provider}/{connection}`: live organization ceiling.
- `connector-authority/{org}/{run}/{warrant-id}`: owner account and independent
  `cell`/`warrant` capabilities with provider, connection, scopes and byte limits.

New connections stay pending until credentials and both live ceilings exist.
Provider-filtered read defaults are materialized explicitly; malformed permissions
never inherit defaults. AI generation/classification are separately budgeted;
“read only” does not mean zero cost. S3/Azure logical bucket/container metadata is
stored in `external_id` and inferred from unambiguous legacy bases.

Migration `0008_connector_authority.sql` tracks released mints by org, connection,
owner account, provider, run, warrant, cell/graph and expiry. Cell mints re-read
the declaration under the same lock as edits; a stale frame cannot restore a
removed grant. Cell edits/deletes revoke prior projections before acknowledgement.
Policy edits close mandatory
live gates and delete tracked run documents before persisting new permissions.
Failed distributed writes are not acknowledged as successful updates; already
closed gates remain deny-all. In-flight broker calls use their fetched snapshot.

The app remains **write-only for credentials**. Its identity needs credential
create/delete, permission-child create/update/delete, and trusted
`connector-policy/` / `connector-authority/` create/update/delete. It must not gain
secret read access. Generated code and runtime callers must never gain trusted
policy write access. Deployment ACL provisioning remains parent/operator-owned.
Health verifies policy namespace write/delete using reserved deny-all `_health`
documents and requires the new authority table.

Rust minting, SQL and vault ACLs are trusted boundaries. Authorization is enforced
again by liaison's private Lean `Authorized`/`Reserved`/`Prepared`/`Resolved`
witnesses and kernel-checked four-ceiling proofs. Tests do not replace those proofs.

## Runtime and writer contracts

The app now provisions local `postgres/compute` and `vault/{graph_id}` records
through `server/local.rs`. Local accounts always use the executing actor UUID;
external shared accounts keep the credential owner UUID. Connection ceilings
derive from the actor's SCRAM role/schema or the bound graph namespace. Local
operation catalogs and advanced ceilings accept `rows.select/insert/update/delete`
and `secrets.read/write/describe/list`; deletion requires an explicit organization
grant. Optional cell selections `compute` and `graph` use the existing structured
`CellConnector.permissions` shape. They are reserved service selections, not
external connection UUIDs. Defaults give DB sinks insert-only authority on their
configured table and secret readers the configured secret names plus metadata-only
listing. Explicit compute selections must remain within the actor schema.

`0008` supports nullable external `connection_id`, synthetic `connection_ref`,
authenticated `owner_id` and a non-secret `projection` snapshot. The snapshot
allows the write-only app to provision conversation/publication refinements;
it contains no credential, warrant tag or plaintext secret. Health checks these
columns and local-service permission namespace write/delete readiness.

Compute setup serializes per organization and provisions a SCRAM verifier, owned
schema and conventional DB-sink JSON value table. Model requests receive only the
non-secret host/port/database/actor schema needed to compile a matching capability.
Compiled queries still consume the runtime's schema/role and four-ceiling proofs.
Registration sends `recoverInputs:true`, retaining incompatible history as source
errors rather than passing it to functions. Caller-owned `inputTypes` metadata
already reaches the kernel-checked source contract.

Writer launch always sends the organization tool list. Narrowing messages send
only the intersection with the live session's current tools; PUT refresh remains
credentials-only. Organization policy edits apply narrowing before acknowledgement.
A later wider organization list cannot restore a removed session operation.

Registration, input feeds and scheduled function calls carry authenticated
`binding` (`org_id`, actual `user_id`, `graph_id`), policy, `liaisonUrl`, and fresh
function-name connector grants. Entries contain `provider`, `connection`, owner
`account`, optional logical `bucket`, `organization`, `connectionPermissions`,
`cell`, and `warrants` (`operation`, warrant, declared cost). Storage cells derive
exact object grants. Messaging intersects a selected cell grant and narrows to
the actual channel or sender/recipient; output text cannot replace selectors.

The execution user and credential owner are distinct for shared org connections.
The runtime must fetch connection policy using the trusted run's owner account,
while retaining the authenticated execution user for DB/tmp confinement. Changing
the execution user to the credential owner is not an acceptable integration fix.
The runtime retains that correspondence while enforcing local effects.

Writer repository credentials use a primary `repositories.read` warrant plus
`operations:[{operation:"repositories.write",warrant:...}]`. The SDK checks every
supplementary token's org/provider/connection/action, selects it only for the named
operation and calls `Body.connector`. Write authority is scoped to the notebook's
`typednotes/{graph-slug}` subtree. Model selectors are component-split; Gemini's
inventory-only `models/` prefix is removed. Credentials are in-memory only.

## Verified native writer and repository modes

The shared catalog matches **54 providers / 165 supported operation-provider
pairs**, with **zero unsupported advertised pairs**. Radius generation is enabled
through the verified Pi/SSE native adapter and production writer protocol checks.
GitHub/GitLab deletion is an explicit advanced operation, excluded from the
read/write preset. Signal/WhatsApp remain send-only: no unsolicited send is used
as a probe, and unsupported history operations are refused.

Writer creation opens the checkout without a model message, receives the actual
persisted Lode session ID, then binds the app-minted run projections before
starting generation. The model projection has
`conversation:{sessionId,allowedTools}`; repository projections have
`publication:{branch,root:["typednotes",graph-slug]}`. Both refinements are stored
by the trusted write-only service, graph-bound, and may not change identity or
widen. Refresh re-checks the live organization/session tool intersection.

`call.context` uses the actual pure `Liaison.Wire.NativeContext` SDK. The broker
derives truthful client/gateway headers. Chat/Messages/Responses/Gemini/Pi accept
bounded inline local function metadata and corresponding reasoning/signature
replay; provider-hosted tools, remote retrieval and model/routing overrides remain
refusals. When a tool retires, `Model.boundedHistory` recovers after its latest
exchange, removes its call/result references and keeps the user task. Fully
permitted opaque replay remains intact; recovery never restores a removed tool
or falsifies the user/agent initiator. Real broker tests cover this for both Chat
and Radius, in addition to full protocol continuation fixtures.

These URL-free repository modes now execute through the actual broker:

- `repositories.read`, resource `[owner,repo]`, payload
  `{"view":"branch","ref":"branch"}`: resolve immutable head; return GitHub
  `commit.sha` or GitLab `commit.id`.
- Same operation/resource, `{"view":"tree","ref":"immutable-commit"}`:
  normalize both providers to a nontruncated `{"truncated":false,"tree":[
  {"path":"…","type":"blob","mode":"100644","sha":"…"}]}`. Subtree
  inventory requires recursive read authority, not an exact repository grant.
  Contents use existing native per-file/base64 reads; no archive/download bypass.
- Same operation/resource, `{"view":"ancestry","ref":"commit","branch":"…"}`:
  verify branch membership and return the existing compare/merge-base shape.
- `repositories.write`, resource `[owner,repo,...project-components]`, payload
  `{"view":"commit","branch":"…","expectedHead":"…","message":"…",
  "changes":[{"resource":["relative","file"],"contents":"UTF-8 or null",
  "mode":"100644 or 100755 or 000000","delete":false}]}`: authorize every
  full changed selector, consume independent deletion scopes, enforce the
  observed head and atomic/fast-forward publication. Return `{"commit":"…"}`.
  Missing atomic scope/head protection is a refusal, never generic Git Data HTTP.

Checkout consumes privately constructed Lean file witnesses carrying selector,
bookkeeping-confinement and regular-file proofs. It rejects symlinks/submodules,
case variants of `.git`/`.lake`, incomplete/oversized trees and existing directories.
Filesystem process isolation remains a trusted boundary. Nested GitLab namespaces
and SHA-256 repositories are explicit unsupported shapes. Native JMAP mailbox
operations require an actual account ID and API endpoint; a session discovery URL
is not an implicit raw-call grant. Drive's advertised `files.create` creates an
empty named file, not arbitrary artifact contents. Unsupported native shapes
remain structured refusals, never raw provider fallbacks.

## Verification

```sh
cargo build -p web --features server
cargo test -p api --features server --lib
# In the local Lake override workspace:
lake build +LodeTest +LunTest.Lun.FetchTest lode:exe lode-native-smoke:exe
# In the app checkout:
python3 scripts/test_native_connectors.py \
  --temp-root "$APPROVED_TEMP_ROOT" --runtime --real-writer \
  --lean-workspace "$LOCAL_LAKE_WORKSPACE"
```

The real app HTTP/broker/PostgreSQL fixture passes nine groups: write-only
provisioning, native probes/classification, selector/payload denials with zero
upstream calls, declaration-bound cell mints/revocation, ledger exhaustion,
revocation, policy failures and audit/cleanup. Eight additional groups execute
app-provisioned SCRAM queries, graph-vault reads/writes, independent four-ceiling
denials, shared-owner HMAC requests, source recovery and live narrowing through
an actual compiled Lun driver. The full writer fixture uses actual compiled
Lode/Workspace and the real broker: checkout, model tool execution, Lake check,
denied ungranted removal, exact-file advanced deletion, native atomic publication,
Lun adoption, and user-editable source type constraints all pass. All Git writes
are disposable local fixture commits/native ref processing; no git-push command
or remote repository is used.

The linked Lean SDK verifies real Chat/Radius context/function-tool transport and
retired-tool history recovery. The writer request-shape fixture separately checks
organization launch tools, narrowing and credentials-only PUT refresh. Lode's
fake-broker suite passes twelve protocol cases plus compaction, in-flight
narrowing/refresh and restart. Those fake peers are not the execution proof.

API tests: **99 passed**. Browser regression: **24 groups passed**, including
source type editing before adoption and correct widgets after rebuild. Liaison's
independent suite passes **655 real HTTP cases** and zero catalog gaps; Lun's
independent suite passes **69 real compiled-driver cases**. The corresponding
Lean tests/proofs and executable builds pass against local sibling overrides.

The coordinated local release set is Typednotes **0.6.0**, Linen **1.10.0**,
Liaison **0.6.0**, and Lode/Lun **0.3.0**. Package locks, image defaults,
generated-driver SDK pins and CI references must match this set. Local commits
and tags do not publish the dependencies or deploy an environment.

Deployment remains operator-owned: publish the sibling tags in dependency order
(Linen, Liaison, Lun, Lode, app), apply migrations through `0008` before the new
app, and install the separated credential/policy/run-projection vault ACLs above.
Rebuild old runtime artifacts: the new runner refuses a cached executable without
`bounded-eff-v1` attestation. No app/native workflow blocker remains in these
fixtures. Container/Linux and live OAuth/paid-provider conformance remain separate
verification axes.
