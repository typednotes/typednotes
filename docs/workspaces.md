# Organizations, projects and notebooks

## Organization

The billing/credits and access unit. It owns members/roles, connections and
notebook permission ceilings. Ledger credits are accounted here; provider charges
still belong to the provider account. An invoicing/payment interface is not yet
implemented. Selected default targets always recheck the user's membership.

## Project

A workspace inside one organization. It groups notebooks, a primary GitHub/GitLab
repository and project-specific messaging interfaces. Generated Lean projects
live beneath `typednotes/{notebook_slug}` on the selected repository branch.
An inbound messaging address routes to one project. Selecting a connection does
not automatically grant message send/write permissions.

## Notebook

The basic work unit: named natural-language cells form a reactive graph. Sources
bring values in, nodes compute, sinks publish results. Lode implements Lean code,
Lun checks/builds/runs it, and the app records private input/outcome history.
Descriptions, inputs and results form the primary view. Implementation, wiring,
type/configuration, model/cost and maintenance controls unfold on demand.

## Guided setup and defaults

Setup is persisted user state, not a separate screen. `/onboarding` bookmarks
redirect to the relevant standard page. A compact guidance strip points to code
connections, repository settings, notebook creation/model selection or permissions.
Saves refresh the guidance, with no query per typed character. The three
collapsible hierarchy boxes are removed; normal titles retain definitions/tooltips.

Migration 0013 atomically gives newly inserted users an owned **Personal
workspace** organization, **My project**, user-bound unique slugs and defaults.
Existing/deleted workspaces are not silently recreated. Invited addresses get
defaults too; welcome credits are idempotently granted at verified sign-in, not
on invitation. Linked IdPs share the same user/defaults.

Choose defaults in Account settings. Creating an org/project/notebook selects
it as the current default. Required repository/model/tool grants remain server
validated; setup never widens organization/connection ceilings automatically.

Preferences persist per user in `user_workspaces`. Removed/foreign pointers do
not authorize navigation: the getter resolves current membership and parent
relationships. Valid completed defaults continue to open their notebook if
generation permissions are subsequently narrowed; execution checks live authority.
Existing ready workspaces can be chosen without recreating resources.
`/organizations` is the organization directory; Account settings contain the
default-workspace selectors.

Optional storage, calendars, email and messaging do not block initial setup.
Help remains keyboard accessible and hierarchy labels have native hover tooltips.
Forging the completion flag at the API cannot skip required fields.

## Selecting a repository

Browse accounts/organizations derived from visible repository names, filter
loaded repositories locally, and load more explicitly. Organizations with no
visible repositories are not invented from inventory. OAuth organization/account
restrictions and application permissions remain independent ceilings.

Direct name/HTTPS entry saves through scoped `repositories.read` metadata. The
broker verifies returned identity; the app also validates the canonical host/URL.
Cross-provider URLs, path/query injection and unsupported nested GitLab namespaces
are refused. Listing failure does not prevent an exact-repository read grant from
being used directly. Native pages are fixed at 100 entries, page numbers are
canonical decimal text 1–100, and oversized inventory replies are denied.

## One connection per provider

An org can create one connection for each provider. Failed/pending connections
also occupy the provider slot; use or remove the existing connection before
replacing it. UI absence is only guidance: storage serializes under an org
transaction lock and a database trigger rejects duplicate insertions. OAuth
start checks availability before redirecting; callback races are still enforced
at storage. Duplicate requests do not rewrite credentials. Historical duplicates
are retained for deliberate cleanup. Existing IDs/grants remain scoped.

## Repository recovery and code writes

`credential_unavailable` means the broker could not read/refresh credential data
or its permission document, not that a write grant is missing. Use **Reconnect**
on the existing connection or the replacement-token disclosure for a code-host
PAT. IDs, project links, scopes and byte bounds remain. Creator/admin authority
is rechecked, old projections close first, failures leave the slot pending, and
a revision fences concurrent repairs. The app remains credential-write-only.
If all connections fail, check the broker's vault identity/availability instead
of enlarging notebook permissions.

Repository selection does not enable writes. Repository settings offers
**Allow notebook code writes**, an explicit creator/admin action granting only
`[owner,repo,typednotes]` descendants. It preserves existing grants/limits, adds
no deletion and refuses a disallowing org ceiling. OAuth/PAT scopes remain an
additional ceiling; runtime and broker still intersect all four authority bounds.

## Automatic code and agent activity

Saving a cell queues generation when required setup is ready. Requests are
durable/actor-bound; checkout survives navigation and launches serialize with
build adoption. Failed setup/generation retains the saved declaration and an
actionable notice. New declarations start fresh bounded writer sessions rather
than widening existing function/service ceilings. **Code activity** shows actual
checkout, assistant/tool updates, file edits, checks and publication details,
not invented hidden reasoning or provider token streaming.

Lode 0.4.3 acknowledges immutable caller-pinned output/source/wiring contracts.
`lun_build` consumes a private checked manifest; Lun proves actual Lean type and
wiring equalities. Unpinned parent outputs may evolve with dependent argument
types, checked through Lean LSP and compilation. Generated signatures/manifests
never replace user pins. Older writers are refused before generation messages.

## Public links

See [public sharing](release-0.9.0.md#public-read-only-sharing) for snapshot
contents, visitor isolation, pure execution, revocation, expiry and limits.
Only UI source inputs are editable. Definition/configuration/secret mutation APIs
remain membership-authenticated; a public token is not an organization session.

## Deleting a workspace or account

[Deletion](deletion.md) explains organization cascades, automatic default resets,
owned-project cleanup, last-owner transfer and retry behavior. Account settings
contains the confirmed account deletion action; existing members can become
owners through Organization settings → Members before a sole owner departs.
