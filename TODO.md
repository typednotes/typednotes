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
