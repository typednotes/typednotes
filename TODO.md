# UX

- [x] Connecting with Google and then Github with the same e-mail should not produce an error, just point to the same account.
- [x] The suggested s3 bucket us scaleway, maybe suggest an AWS bucket instead
- [x] AI providers should be selected in a menu and ordered by alphabetical order.
- [x] I should be able to connect Drive, Dropbox, Azure along with S3
- [x] The org id should not fail on duplicate, it should be validated/rejected upfront
- [x] There should be a notion of Project
- [x] I should connect to github, or gitlab and select a primary repo for the project
- [x] Add interfaces to Whatsapp, Signal and Slack
- [x] The black background makes the log-out button invisible.
- [x] The choice of color is a bit weird, nothing happens on button hover, can you make the design look more responsive, modern and functionnal ?

# Computations

The design is [`docs/computations.md`](docs/computations.md) (with
[`docs/services/lode.md`](docs/services/lode.md) and
[`docs/services/lun.md`](docs/services/lun.md)); the work below is the app's
share of it, in dependency order. Source kinds: `scheduled`, `watch`, `ui`,
`secret`, `endpoint`, `channel`. Sink kinds: `db`, `http`, `storage`, `ui`,
`channel`.

- [x] Org members: owners and admins can add a user to the org by email (an `owner`/`admin` acts on `memberships`, which exist in the schema but have no UI) and remove one.
- [x] `migrations/0004_computations.sql`: `graphs`, `graph_cells` (kind `node`/`source`/`sink`, description, config by trigger or kind, impl), `graph_inputs` (the input log that re-registers a lost lun session and that `watch` dedupes against), `graph_updates` (what changed, per update), `source_checks`, `endpoint_calls`.
- [x] lode and lun HTTP clients (`packages/api/src/server/`): open/follow/steer a lode session with fresh warrants per message; submit a lun build from the published `lun.json`; register/update sessions with the `(org, user, graph)` binding and, for `storage` sinks, attached warrants.
- [x] Warrants for lode (repo `write` on the project's branch, model per-call budget), lun (repo `read`) and `storage` sink writes (the `connections.md` §7 shape; channel sink sends reuse the existing messaging path).
- [x] Notebook UI: the cells in order (description, config, the implementation lode produced, last outcome), "implement" and "steer" against lode, inputs and changed nodes against lun, with sources, sinks and the closure of the dependencies displayed. `ui` sink renderers: table, chart, markdown-through-a-sanitizer — never raw HTML from the graph.
- [x] Source scheduler: cron due times, the tick that calls each due cell's function on lun and feeds the input; `watch` dedupe against `graph_inputs`; per-org rate cap; SSRF guard on URLs; audit in `source_checks`. Signal channel pulls ride the same tick.
- [x] `ui` source cells: a widget per input (the widget follows from the input's JSON type lun reports), feeding the session, recorded in `graph_inputs`.
- [x] `secret` cells: a write-only password field; the value goes to the vault at `secret/data/graph/{org_id}/{graph_id}/{name}` (app `create`/`delete`, `lun` `read`); functions declare `SecretStore`.
- [x] `endpoint` cells: `POST /hooks/graphs/{token}` (token stored hashed, looked up by hash, rotatable; URL shown once); body feeds the input; the answer is the changed nodes; per-endpoint rate cap; audit in `endpoint_calls`.
- [x] `channel` cells: sources feed new `channel_messages` rows (the message object) to the session; sinks send a changed node's text through the existing liaison send path and record the outbound message.
- [x] Per-user compute: provision schema+role `{org_slug}_{user_id}` on `compute-db` (SCRAM verifier, never plaintext across Postgres), credential at `secret/data/compute/{org_id}/{user_id}` in the vault.
- [x] Re-registration: when a lun session is gone (container recycled), rebuild it from `graph_inputs` before the next update.
- [x] `LODE_URL`, `LODE_TOKEN`, `COMPUTE_DB_URL` in config and health (the status line on the home page) — and `LUN_URL`, `LUN_TOKEN`: the app submits builds and drives sessions itself.

The coordinated 0.6.0 app / 0.3.0 runner and writer / 0.6.0 broker contract
is verified through the real local pipeline, including bound DB, graph secrets,
storage, independent permission denials and source recovery. See
[`docs/native-connectors.md`](docs/native-connectors.md) for the contracts,
kernel-checked guarantees, fixture coverage and deployment requirements.

# UI

- [x] in the notebook like UI, the user refers to other cell by name to use them as input (you can be inspired by observable).
- [x] A cell can depend on many other cells or none (source)
- [x] in the notebook, the cells are sorted in declaration order (they have a declaration number), but can be sorted by name or topological order (then name or id).
- [x] an alternative presentation of cells would be a graph whith edges materializing dependencies.
- [x] The natural language description in the cell yields code and a lean 4 type for the cell fn, the user may force the type (input types are forced by arguments but output could be constrained).
- [x] The user is constrained to output Eff T where Eff guarantee a list of hard constraints and other set by the organization, including.
  - only read-write your DB schema (this should be relatively transparent to the user)
  - can only read/write a rsserved folder (for org/user) on disk (temporary in any case)
  - can only query some domains
  - can call tool x or y
  - can use this AI or that messenging app
- [x] The Effect constraint is enforced at the lun level
- Be extensive and consistent about Eff level  permission. Each connector potentially comes with potentially rich permission. including S3 storage, drive, calendar, e-mail etc. Make simple things easy to set with sensible defaults
  - [x] Organization provider ceilings, connection presets and per-cell grants have structured operation/resource editors, quick boundaries and request/response limits. Storage cells can infer an exact object from their configured path.
  - [x] Real browser lifecycle coverage: create, rename with dependent reference updates, delete/refuse deletion, sort with stable draft values, dependency graph keyboard navigation, type edit, regenerate one/all, feed values, errors, dependency change and session recovery. Real app/API/PostgreSQL; writer/runtime/broker/vault mocked. See [verification and remaining integration](docs/notebook-ui.md).
  - [x] Verify the actual native adapter/runtime path and Lean authority proofs for all advertised connector operations; 655 real broker HTTP cases cover 54 providers / 165 pairs, with zero catalog gaps, plus 69 compiled-driver runtime cases and the full app/writer/broker/runner pipeline. Remote paid-provider conformance and build/transport trust boundaries remain explicit in the linked contracts.
  - [x] Validate forced source input types in caller-owned graph build metadata and provide recovery for incompatible recorded inputs after a source type change; Output/Wiring/Source contracts are kernel checked and real adoption/feed/recovery cases pass.

# Graph

- [x] Can a node depend on multiple node themselves depending on one node — yes; the real compiled diamond fixture joins two derived nodes sharing one upstream. [Details and evidence](docs/graph-and-lode.md#can-a-node-depend-on-derived-nodes-sharing-one-upstream).
- [x] Can a node yield several times ? (it should be able like Reactive / Observable) — yes across live-session input occurrences; three emissions, duplicate suppression and error recovery are verified. [Exact stream semantics](docs/graph-and-lode.md#can-the-same-node-emit-several-times).

# Lode

- [x] Has Lode enough doc about graphs to code them? — the prompt carries graph/function/effect contracts, examples, manifests and the real verification workflow. [Answer](docs/graph-and-lode.md#does-lode-have-enough-graph-documentation).
- [x] Does Lode have enough tools to develop the code? It should have access to the same Eff restricted interfaces as the code itself (DB, webcall, etc) — implemented authenticated, proof-bounded runtime trials through the real Lun/broker path; DB, secrets, files, HTTP and connectors verified. [Bridge guarantees](docs/graph-and-lode.md#does-lode-have-the-same-restricted-eff-interfaces-as-its-implementation).
- [x] How does Lode call these tools? (it could write the code for each) — finite parsed tools consume permission witnesses; bounded Lean trial functions use input-only `lun_call` with private caller authority. [Dispatch](docs/graph-and-lode.md#how-does-lode-call-tools).
- [x] How does Lode test the code is working as expected? — real `check`, `lsp`, `lun_build` and authorized `lun_call`, with compiled positive/negative integration fixtures. [Testing](docs/graph-and-lode.md#how-does-lode-test-generated-code).
- [x] Does Lode have access to a Lean 4 LSP server? — implemented bounded `lsp` diagnostics/hover/definition/completion/goals; 63 real dispatcher calls pass. [LSP](docs/graph-and-lode.md#does-lode-have-lean-lsp-access).
- [x] Can Lode search for the code in the repository? I guess after the repo is cloned locally with File utils (it should have the basic tools to search, read and edit files but maybe with). — checked `ls`, `grep`, `read`, `write`, `edit` and LSP navigation, within the current agent/session permissions. [Repository tools](docs/graph-and-lode.md#can-lode-search-read-and-edit-repository-code).

# CI problems

- [x] pushing a tag if tests are not finished results in an error. Can't we wait for the CI to finish to deploy if a tag was pushed ? — bounded exact-commit main-CI polling, with failed/invalid evidence refused; 35 offline gate cases. [Policy](docs/workflow-policy.md).
- [x] Can some of the very long runs be made optional to make the CI run shorter? — browser/branding checks are opt-in for PR/manual runs; main still runs the complete release suite in parallel jobs.

# UI problems

- [x] On https://app.typednotes.com/orgs/grislain/projects/project-1/settings/repository I cannot set a repo because: "Could not list repositories: connector operation is not permitted by the live ceilings" — the live org has only SecretStore enabled; implemented precise ceiling diagnostics and a policy-editor link. Real broker listing/denial/admin-correction cases pass.
- [x] Some links such as https://app.typednotes.com/orgs/grislain/settings/connections take some time to react — internal links use client routing; optional health/settings loads no longer suspend navigation. Browser verifies the document stays mounted.
- [x] In https://app.typednotes.com/orgs/grislain/settings/connections I cannot test my baseten connection. I get: "connector operation is not permitted by the live ceilings" — same disabled Connector effect; explicit recovery controls and real app/broker Baseten probe coverage.
- [x] None of the connection works — shared policy blocker is explained before testing, with explicit owner/admin editing; connection provisioning now also honors the existing ObjectStore ceiling consistently. Permissions are never automatically widened.
- [x] Can you explain in the docs how to setup the connection to the calendar and gmail? — [Google setup and troubleshooting](docs/google-connections.md).
- [x] For each AI provider the model should be selected from a list (the list is loaded from the provider). — native broker models.list feeds connection pricing, notebook and classifier menus; saved selection/reload and provider parsing verified.
- [x] Notbook permissions are not so easy to set https://app.typednotes.com/orgs/grislain/settings/notebooks. Please use checkboxes or radio buttons or menu where needed. Please enable a TOML input in a text area. — checkbox grants, radio domain policy and strict TOML/visual round-trip, persisted only on Save. [Guide](docs/notebook-ui.md).
- [x] each new character in an inpput or text area should not trigger a query to the server. wait for submission or idle time. — edits stay local; slug/pricing lookups wait 400 ms and cancel stale timers. Browser verifies one query per typing burst.
- [x] The UI should be consistent, use the same panel width and proportions everywhere. — shared page-width/spacing tokens and corrected column-form sizing; browser checks equal notebook/settings/org widths and 390px layouts.

These changes are local until deployed. To restore the intended live Grislain
connection access, its owner/admin must enable Connector in Settings → Notebooks
and save. Its writer tools are also currently empty and must be selected for
code generation. The implementation preserves both ceilings rather than silently
restoring defaults.

# Dev experience problem

- [x] Is there a way to test the app locally and iterate on it by linking it to other prod services? — private environment launcher, worker-visible transport addresses and optional background scheduling. [Local/remote development](docs/local-development.md).

# UX problems

- [x] When selecting a repo on github, a selector should show other orgs and repo within each orgs.
It should be possible to fill in the string. — account/organization grouping, bounded page loading and direct name/HTTPS selection; exact metadata/resource checks at app and broker. [Guide](docs/workspaces.md#selecting-a-repository).
- [x] Can you make sure other selection mechanisms work. — controlled values, reset/stale-account handling, real-trigger labels and scrollable menus; browser restores model/input selections after hydration and sorting.
- [x] When a connection exist it should not be able to create a new connection (the creation UI widget disapears) — one provider/org, including failed/pending connections; hidden forms, OAuth preflight, serialized server storage and database insertion guard. Concurrent real API creation allows only one winner.
- [x] A user should have a default org and a default project and land on a Notebook — persisted per-user defaults, membership/parent revalidation and direct sign-in landing; deleted/foreign targets never authorize navigation.
- [x] When logging in for the first time, the user should be forced to setup the necessary field by leading it through the interface with tooltips. He may be invited to set optional settings too. — resumable required setup and server completion checks; optional integrations stay skippable and existing connections are reused.
- [x] Can you define in the doc and inline (with tooltips and text) whet each are, e.g. (correct if I'm wrong): — [workspace definitions](docs/workspaces.md), inline text, native tooltips and keyboard-accessible explanations.
  - Org, the invoicing unit, where connectors are defined and user/permissions
  - Project, backed by a git repo and has its own messaging interfaces
  - Notebook, the basic work unit.
- [x] A notebook should look simpler and the complexity should unfold if necessary. — setup, writing notes, view options, cell configuration/code/wiring, activity and maintenance controls unfold on demand; inputs/results stay visible.
- [x] A use should be able to share a notebook through a link in read-only mode (only user input should be editable). — anyone-with-link snapshots, isolated expiring viewer sessions, UI-input-only API, owner/visitor isolation and revocation. Public responses exclude secret cells/nodes; Lun consumes proof-checked Trace/Error-only execution. [Sharing contract](docs/release-0.9.0.md#public-read-only-sharing).

Verified locally: 106 API tests, 33 browser groups, 675 real broker HTTP cases,
12 app/broker/SQL groups, 9 compiled app/runtime groups, Lean suites and offline
fleet checks. Rollout needs app v0.9.0, Liaison v0.6.3, Lun v0.3.1, and fleet
v0.6.1 adopting migrations 0009–0011. [Publication/deployment order](docs/push-order.md).

- [x] When trying to delete an Org, I get: ```database error: error returned from database: update or delete on table "orgs" violates foreign key constraint "credit_ledger_org_id_fkey" on table "credit_ledger" at line 2621```. When deleting an org all connections to the org should be removed in cascade and org-less user should be back to the onboarding state, just without the default org and project. — append-only ledger cascades, locked authority/session/vault/compute cleanup and full default/completion reset. Older ledger FKs are refused before credential deletion. [Deletion guide](docs/deletion.md).
- [x] When deleting a user, if a project becomes without an owner, it is also destroyed, same for orgs, an org with no user is deleted and the related entities are deleted in cascade. — confirmed self-account deletion, owned-project/empty-org cascades, identity/session/credential cleanup and other-owner/tenant preservation. As agreed, a sole owner with other members must transfer ownership first; the Members UI can promote an existing member. [Account deletion](docs/deletion.md#delete-your-account).

Deletion verification: 106 API tests, 41 browser groups, 13 real app/broker/SQL
groups, 9 compiled app/runtime groups, Ledger Lean tests and offline fleet checks.
Requires app migration 0012 and ledger migration 0003; release refs are app
v0.9.1 / ledger v0.3.7 / fleet v0.6.2. These changes are local until published/applied.

- [x] The 3 collapsable boxes in https://app.typednotes.com/onboarding with Organization, Project, Notebook are weird. — legacy route redirects to normal settings; the separate wizard/boxes are removed. [Workspace guide](docs/workspaces.md).
- [x] When connected to a repo, the listing of orgs/repo under a user fails: `Could not list repositories: the credential broker refused the call (502 credential_unavailable) Review notebook permissions` — credential-specific diagnostics and stable-ID OAuth/PAT repair preserve project links and authority. Real broker failure/repair and signed-in repository listing verified. [Native connectors](docs/native-connectors.md).
- [x] The setup process should not be based on a new screen, it is just a state of the user telling that you should be guided through the standard UI to set things up. — persisted guidance/defaults drive compact prompts on existing pages, including after hydration/reload. [Workspace guide](docs/workspaces.md).
- [x] Out of the box a default org and a default project are created when the user is created. — migration 0013 atomically creates owned defaults on user insertion; existing/deleted workspaces are not silently recreated. Actual SQL/browser paths verified.
- [x] No code ie written on cell creation, I should be able to see the agent thinking, cloning the repo, writing code and pushing it. — durable actor-bound automatic generation and actual logged agent/checkout/edit/check/publish activity; real compiled writer publication/adoption verified. [Notebook workflow](docs/notebook-ui.md).
- [x] When I hit "Generate all code" in https://app.typednotes.com/orgs/nicolas/projects/test/graphs/test I get: `Connection permissions do not allow repositories.write for this resource. Its creator or an admin can edit the connection's operations and resource boundary.` — explicit creator/admin setup grants only notebook-subtree writes, preserves byte limits, adds no deletion and respects organization ceilings. Browser and real broker checks pass.
- [x] Lode, the coding agent, should be able to type-check the graph with a Lean 4 LSP server. In particular if implementing a node implies to change slightly the output type of the parent it can do so (unless the type is set/pinned by the user.) — real LSP parent/child checks plus private caller-pinned manifest witnesses; actual writer lun_build cannot replace a caller String pin with Nat. Unpinned parent changes remain allowed. Requires Lode v0.4.3.
- [x] Once compiled, a lun graph should be very fast, can you check how many QPS a small graph taking 2 numeric input and outputting the result of an arithmetic operation can handle. — 1,800 validated HTTP requests per before/after run, zero errors; reproducible benchmark and source-hashed snapshots. [Measurements](docs/lun-throughput.md).
- [x] When a graph is loaded in lun, could it be kept as in a cache to make QPS much higher? — bounded loaded workers/immutable graph templates with fresh request authority, framing/correlation proofs, deadlines and retirement without replay. Graph throughput reaches 2,339.51 QPS locally (8 clients), median 1.79 ms at one client. Actual runtime/actor/denial and abrupt-parent cleanup checks pass. [Cache and rollout](docs/lun-throughput.md).

Current workflow verification: 106 API tests, 42 browser groups, 15 real app/broker/
SQL groups, 9 compiled app/runtime groups, full compiled writer/pinned-type/attenuation
pipeline, 68 compiled Lun runtime cases, 66 real LSP dispatcher calls, Lean suites
and offline fleet/workflow gates. Features are verified locally; production rollout
requires app v0.10.0 / Lode v0.4.3 / Lun v0.3.2 / fleet v0.6.3 and migration 0013.
Publish Linen v1.11.0 first, then its dependent Lun runtime. Lun's Linen requirement
and immutable lock reference the exact local release commit; normal locked builds
were verified before tagging. [Release preparation](docs/release-0.10.0.md).
