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

`/onboarding` chooses or creates an organization, project and notebook, then
connects the required code host and generative AI provider. It reuses connected
providers. Repository read/scoped subtree write grants, an allowed model and
required writer tools must pass server checks before setup completes. The wizard
never expands organization or connection ceilings automatically. Members who
cannot edit a required ceiling are directed to their org admin.

Preferences persist per user in `user_workspaces`. Removed/foreign pointers do
not authorize navigation: the getter resolves current membership and parent
relationships. Valid completed defaults continue to open their notebook if
generation permissions are subsequently narrowed; execution checks live authority.
Existing ready workspaces can be chosen without recreating resources.
`/organizations` is the organization directory; account settings link to default
workspace setup.

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

## Public links

See [public sharing](release-0.9.0.md#public-read-only-sharing) for snapshot
contents, visitor isolation, pure execution, revocation, expiry and limits.
Only UI source inputs are editable. Definition/configuration/secret mutation APIs
remain membership-authenticated; a public token is not an organization session.
