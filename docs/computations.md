# Computations

**Status:** native app/writer/runtime contract implemented and locally verified · **Last updated:** 2026-09-30

**Coordinated release target:** Typednotes 0.6.0, Lode/Lun 0.3.0,
Linen 1.10.0 and Liaison 0.6.0. Package/version/default pins, deployment and
release publication remain parent/operator-owned.

The product: a notebook where each cell is a **natural-language description of a
node** of a reactive graph — like a Jupyter notebook whose cells are prose, and
whose outputs update like a spreadsheet's. The cells are implemented as Lean
functions by `lode`, built and run by `lun`, and the user watches the graph
react. The spreadsheet of the AI era is the framing; the honest engineering
underneath is: **the repository is the source of truth, the graph is typed, and
the agent's authority is bounded by the same warrants as everything else.**

Per-service detail: [`services/lode.md`](services/lode.md) (the implementer) and
[`services/lun.md`](services/lun.md) (the runtime). The vocabulary of proof tiers
is [`proof-strategy.md`](proof-strategy.md).

```
 the app ──(implement: repo `write` + model warrants)──────▶ lode ──publish──▶ the project's repo
 the app ──(build: repo, branch, commit + `lun.json`)─────▶ lun ◀── fetches the commit ──┘
 the app ──(register a session; feed inputs)───────────────▶ lun
 lun ──(`PostgreSQL` effect, as the (org, user) role)──────▶ compute-db
 lun ──(read, its own vault identity)──────────────────────▶ secret/data/compute/{org_id}/{user_id}
  lun ──(native storage operations + scoped warrants)───────▶ liaison ──▶ S3 / Azure / Dropbox
 lode, lun ──(warrants, liaison's wire format)─────────────▶ liaison ──▶ GitHub / GitLab, the model
```

---

## 1. The model

A **graph** belongs to a project ([`connections.md`](connections.md) §10), and a
project's primary repository is where its code lives. A graph is an ordered list
of **cells**, each a natural-language description of one node:

- a plain **node** — "double the amount", "look up today's EUR→USD rate",
  "format the total in euros";
- a **source** — how the graph learns about the world. Six kinds (§3):
  **scheduled** ("fetch https://example.com/prices every hour and read the
  first table"), **watch** ("tell me when that page's price changes"),
  **ui input** ("let me pick the country"), **secret** ("an API key for the
  pricing service, that I set once and never see again"), **endpoint**
  ("receive GitHub's push webhooks"), and **channel** ("when a message
  arrives on the project's WhatsApp number, that's the order");
- a **sink** — where a result lands. Five kinds (§4): **db** ("write the total
  to a `totals` table"), **http** ("POST the alert to the incidents service"),
  **storage** ("archive the report to the org's bucket"), **ui** ("show
  the invoice as a table in the notebook"), and **channel** ("send the total
  back on the project's Slack channel").

The cells are the user-facing artifact. Everything else — modules, functions,
signatures, the `lun.json` — is **derived and disposable**: it lives in the
repository because that is where code is reviewed, diffed and versioned, but the
notebook in Postgres can always re-derive it. This is what makes lode and lun
disposable too: the app holds the only state that cannot be reconstructed.

### 1.1 The node contract

`lode` implements each cell as a named function of [`lun`](services/lun.md)'s
declared contract, and the user's description fixes its shape:

- `fn : A → Eff effs B` — every argument and the result are **JSON values**
  (Lean `FromJson`/`ToJson` instances), so a node is callable, inspectable and
  storable without Lean knowledge;
- `effs` is a **row of vetted effects** — the function's declared authority:
  a node that only computes declares `[]`; a `scheduled` or `watch` source's
  function declares `HTTP`; a `db` sink declares `PostgreSQL`; a function that
  reads a secret cell declares `SecretStore`; a `storage` sink declares
   `ObjectStore` for S3/Azure or native `Connector` for Dropbox. General connected
   service operations use scoped `Connector` requests. A `ui`, `endpoint` or `channel` source compiles to no function
  at all — it is an input with a trigger (§3) — and a `channel` sink's node is
  plain (`[]`): the sending is the app's, after the recompute (§4.5).

The graph program wires the functions together in linen's `Reactive` monad:
wiring a function to a value of the wrong type does not compile, the graph is
acyclic by construction, and a failing node's error stays with its node — its
dependents are skipped, everything else carries on. The notebook UI displays,
per cell, the sources, the sinks, and the dependencies (direct and closure)
that `lun`'s build answer reports for the graph.

The app additionally supplies caller-owned `outputType`, `inputTypes` keyed by
configured source name and ordered named `dependencies`. Lun's `OutputContract`,
`SourceContract` and `WiringContract` are kernel checked, and runtime witnesses
bind those contracts to the actual graph. Organization effect/domain policy,
connection permissions, cell capability and warrant ceilings are enforced at
execution; generated code and session refresh cannot widen them.

### 1.2 What the user sees

The notebook page shows the cells in order, each with its description, the
implementation `lode` produced for it (function name, signature, effects), and —
once the graph is built — the node's last outcome. Inputs appear as `ui` cells
with widgets (§3.3); a change feeds the session and only what depends on it
runs again. The implementation log (lode's session) is followed in the same
page while it runs.

Each cell has a life of two phases, shown on the cell:

1. **lode writes its code** from the description — the cell says so, and
   expands to follow the writing: the steps of lode's run that mention the
   cell (the files it writes, `check` and `lun_build` results) and the
   unpublished code (the hunks of lode's diff that mention it);
2. **lun runs it** — the cell shows its outcome, and expands to the code the
   published commit holds for its function.

A cell goes back to phase 1 when the code fails — lun cannot build it, or
reports an error for the cell's node — or when a member reports that it does
not do the right thing ("Not doing the right thing?"): a new lode run
rewrites that cell's code with the reason, while the current code keeps
running on lun until the rewrite is built. A description edited after its
code was written marks the code out of date, with a one-click rewrite.

## 2. From description to running graph

Three steps, all driven by the app, all audit-attributed to a run:

1. **Implement — `lode`.** The app opens a lode session over the project's
   primary repository (a GitHub or GitLab connection of the org, through
   liaison, §5). Launch carries the organization tool list and initially no model
   message; the app obtains the actual persisted session ID, binds trusted
   `conversation:{sessionId,allowedTools}` and
   `publication:{branch,root:["typednotes",graph-slug]}` run projections, then
   starts the cell-description message. Repository credentials carry primary
   **`repositories.read`** and supplementary **`repositories.write`** tokens;
   removals need independent **`repositories.delete`** scopes. The model receives
   an **`inference.generate`** warrant and Lun receives repository read authority.
   `lode` writes the modules, the `lun.json`, publishes **one commit on
   the shared branch** (never a force push), then `lun_build`s the published
   commit and fixes what lun reports per function and graph. A session is done
   when the published commit builds clean and the functions answer as intended.
   Native checkout reads immutable trees/files, not archives; publication consumes
   broker-owned expected-head CAS on GitHub/GitLab, not generic HTTP or racy commits.
   The user steers in natural language mid-run; the notebook re-implements any
   cell by a follow-up message.
2. **Build — `lun`.** The app submits the build (source: repo, branch, commit,
   project path; the functions and graphs from `lun.json`). Same request, same
   build id: a restarted lun re-derives it from the repository alone.
3. **Run — `lun` session.** The app registers the graph as a live session with
   the initial inputs and a **binding `(org_id, user_id, graph_id)`** — what
   lun's `PostgreSQL` and `SecretStore` handlers resolve private targets against
   (§3.4, §4.1) — plus organization policy and fresh operation-specific grants
   for each function, including actor-bound compute/graph-vault and external
   connections. The app provisions all three live policy/run documents before
   releasing warrants. Registration uses `recoverInputs:true`: historic values
   incompatible with a changed source type remain editable source errors and
   block dependents. An invalid new feed is refused before any node executes.
   An update feeds only the inputs it names;
   lun recomputes only what depends on them and answers with the nodes that
   changed. **The app records every input it fed** — a lun session is lost when
   lun's container is recycled (§6), and the app re-registers it lazily with
   the recorded state.

Nothing here needs lode or lun to be durable. lode's sessions and lun's builds
and sessions live on their containers' local disks and are **reconstructible**:
the notebook re-derives lode's work, the repository re-derives lun's build, the
app's input log re-derives the session.

## 3. Sources: how a graph learns about the world

A source cell is one of six kinds — each a different **trigger** with the
same contract: what it produces reaches the graph as the **input it declares**.
The trigger is structured config next to the description, because prose is for
the graph's author, not for a scheduler, a widget or a URL.

| Kind | Config | Triggered by | What happens |
|---|---|---|---|
| `scheduled` | `{url, schedule, input}` | the app's scheduler, at the cron times | the cell's function (`HTTP`) runs on the latest ready build; its output feeds the input |
| `watch` | `{url, schedule, input}` | the app's scheduler, when the extracted value **changes** | same polling as `scheduled`; only a change is fed |
| `ui` | `{input}` | the user, in the notebook | the widget's value feeds the input |
| `secret` | `{name}` | nobody — a capability, not a value that flows | functions that declare `SecretStore` read it at run time |
| `endpoint` | `{token, input}` | anyone holding the URL, over HTTP | the request's JSON body feeds the input |
| `channel` | `{channel_id, input}` | a message arriving on the project's messaging interface | the message feeds the input |

### 3.1 `scheduled`: checks of web pages

- The app keeps a due table and a **tick**: for each due source, it calls the
  cell's function on the latest ready build (`POST /v0/builds/{id}/functions/…`)
  — a function with `HTTP` in its row — then feeds its result to the session as
  the input the cell declares (`POST /v0/sessions/{id}`). The fetch itself runs
  inside `lun`, in the function, where the description says it should.
- The `schedule` is a cron expression (five fields), not an interval: "every
  weekday at 9" is `0 9 * * 1-5`, and the app computes due times from it — no
  external cron service, just the tick and the due table.
- Each check is **audited by the app** (source, outcome, latency, what input it
  fed), even though the egress itself happens in lun — the app is the only thing
  that decides *when* a source runs.
- **Rates, not metering:** v1 caps checks per org per hour instead of charging
  for them (they are our compute, not a paid provider call). A runaway source
  loop is bounded by the cap, like the ledger's two caps bound a runaway agent.

**Egress and the chokepoint — read this as a known weakening.** The broker
(`liaison`) is the sole egress chokepoint *for credentialed calls*
([`architecture.md`](architecture.md) §4.2). A `scheduled` or `watch` source
fetching a public web page is an **anonymous** fetch: no credential exists to
leak, and the vault holds nothing for it. It goes out from `lun` directly, like
`lun`'s own repository fetches do. What is *lost* versus routing it through
liaison is per-call metering and a per-call audit row; the mitigation is that
the app initiates and audits every check and the rate cap bounds the volume. Any
source that needs a **credential** (a private API, an authenticated feed) is out
of scope in v1 and must wait for liaison to take anonymous-or-warranted fetch
requests (§8) for arbitrary authenticated fetches. Existing supported connected
service operations can instead use their native `Connector` adapters; there is
no raw credentialed-HTTP fallback. The anonymous `HTTP` handler now consumes
static URL/method, organization domain and standard-port evidence, validates all
DNS answers and pins a public numeric address while preserving Host/TLS identity.
It follows no redirects and rejects forbidden headers/noncanonical paths.
The app also checks configured URLs; runtime pinning closes the re-resolution
gap. These paths are tested; socket/TLS correspondence remains trusted.

### 3.2 `watch`: when a page changes

The trigger is the **change**, not the schedule: "tell me when this page's
price changes" polls like a `scheduled` source but only feeds the input when
the extracted value differs from the last one fed.

- What counts as "the value" is the description's job: the cell's function
  fetches the page and **extracts the salient part** (the price, the headline,
  the first table) — a changed ad banner or timestamp does not count as a
  change, because it never reaches the input.
- The change detection needs no second mechanism: the app compares the
  extracted value against the last one in `graph_inputs` (§7) and feeds only a
  change; the reactive graph's own equality — an input set to its current value
  runs nothing — is the backstop, and it is also what makes §3.5's retried
  webhooks harmless.
- The audit story separates the two things a user wants to know about a watch:
  `source_checks` records every poll (how many, when, did they fail), and a
  feed recorded in `graph_inputs` *is* the change. The notebook shows both.

### 3.3 `ui`: a widget per input

The widget follows the caller-owned source type (or the adopted implementation's
type when no source constraint was set): a number gets a stepper, a string a text field, a boolean a toggle, a
closed set of string literals a select. Every change goes through the same
session update as any other input and is recorded in `graph_inputs` (§7), so a
re-registered session comes back with the user's last values; only what depends
on the changed input runs again.

### 3.4 `secret`: set once, never seen again

- The widget is a password field, **write-only**: the app records that the
  secret is set, never the value. There is deliberately no "show".
- The value lives in the vault at
  `secret/data/graph/{org_id}/{graph_id}/{name}` — the app's policy is
  `create`, `delete` on that prefix (it writes and cannot read back), and `lun`
   reads it per authorized operation as its own vault identity, exactly like the compute
  credential (§4.1).
- A function reaches it through the vetted `SecretStore` effect: the capability
  separates `describe` from `getValue` as distinct permissions, names are
  segments so a prefix can be scoped, and the value is linen's opaque
  `Secret.Value` — no `ToJson`, no rendering — so **it cannot land in a node
  output, a log line or a `lun.json` by accident.** lun's handler grants
   operation-scoped access only within the graph's granted names. The app's
   local-service minting re-reads the declaration and secret inventory; read,
   describe, write and metadata listing remain separate permissions.
- **The honest boundary:** the user's own function can still build an output
  from the secret deliberately — it is their secret, in their graph, run as
  them. What the design removes is every *incidental* leak path: not in the
  repository, the graph program, a node output, an app table or a log.
- Secrets are **graph-scoped**: members who can run the graph share them.
  Per-user secrets would follow the §4.1 pattern if a product need appears.

### 3.5 `endpoint` (webhooks): a REST URL the world can call

The cell gets an unguessable URL — `POST {PUBLIC_URL}/hooks/graphs/{token}`
— and the URL is the capability, like a lun session id. The app serves the
route for the same reason it serves the Slack and WhatsApp webhooks
([`connections.md`](connections.md) §11): it is the piece that knows the
graph, records the delivery, and holds the input log — lun's sessions stay
pull-only. Webhook senders — GitHub, n8n, IFTTT, anything that can POST JSON
— need no integration beyond being given the URL.

- The request's JSON body feeds the input the cell declares; a body that is
  not that input's JSON type is an error — recorded, session untouched.
- The token is 32 random bytes, **stored hashed and looked up by its hash**
  (the session-cookie pattern, [`connections.md`](connections.md) §2): the
  full URL is shown once at creation, and rotating the token revokes it.
- The answer is the session update's — the nodes that changed — so a caller
  can use it synchronously (feed and read what changed) or ignore the body
  and treat it as a plain webhook. Whether a write-only mode (a `202` with no
  outputs) should exist is an open question (§8): as specified, whoever holds
  the URL can read what changed.
- **Idempotency is the reactive graph's own:** an input set to its current
  value runs nothing, so a retried webhook with the same body recomputes
  nothing. What the retry still costs is a request; a per-endpoint rate cap
  bounds that. Every delivery is audited (§7, `endpoint_calls`) — who could
  call is known by construction: whoever holds the URL.

### 3.6 `channel`: a message is the trigger

Supported inbound messaging interfaces ([`connections.md`](connections.md) §11 —
Slack and WhatsApp webhooks) are addresses routed to the project;
a `channel` source cell binds one to the graph: "when a message arrives on the
project's WhatsApp number, that's the order."

- The app feeds the **message object** — `{text, peer, peer_name, at}` — as the
  input the cell declares; the prose says what to make of it, and `lode` wires
  the parsing node downstream ("the first number in the text is the amount").
- The delivery machinery is the inbox's, unchanged: the same signed webhooks
  (Slack, WhatsApp), the same dedupe — `channel_messages` is unique per
  `(channel, direction, external_id)`, so a retried webhook records nothing new
   and feeds nothing new. Signal's unsupported legacy history/pull path is
   explicitly refused; native Signal/WhatsApp connectors remain send-only.
   **The audit trail is `channel_messages` itself**,
  the table the inbox already keeps.
- Authentication is the webhook's (the signing secrets the app already checks);
  a `channel` source adds no new inbound surface — the addresses were already
  reachable, only their routing changes.

## 4. Sinks: where a result lands

A sink cell is one of five kinds. A sink fires only when its dependencies
change — and every change comes from a source (§3) or the user — so sink
frequency is **bounded transitively** by the sources' rate caps; no sink needs
one of its own.

| Kind | Config | Effect | Where the result goes |
|---|---|---|---|
| `db` | `{table}` | `PostgreSQL` | the per-user schema (§4.1) |
| `http` | `{url}` | `HTTP` | a POST of the node's JSON result (§4.2) |
| `storage` | `{connection_id, path}` | S3/Azure `ObjectStore`; Dropbox `Connector` | native contents upload, **through liaison** (§4.3) |
| `ui` | `{format}` | — | rendered in the notebook (§4.4) |
| `channel` | `{channel_id}` | — (app-side) | sent as a message on the project's interface (§4.5) |

### 4.1 `db`: the per-user database

> "For each user a database with the name of the org and user is reserved, and
> code can only access it, enforced by effects."

Users share orgs, so **isolation is per (org, user)**, and the unit is a
**schema and a role**, not a database: Scaleway Serverless SQL instances are
managed resources — one per declared database, billed each, minutes to
provision — and the app's identities hold data rights only. Creating one per
user would put provisioning on the cloud API, beyond the app's rights, for a
property Postgres already has at finer grain.

**The shape.**

- **`compute-db`**, a second Serverless SQL database, declared in
  `typednotes-infra` like the others. **No service table ever lives in it.** It
  exists to hold user sink data and nothing else.
- Per (org, user), lazily on first use (a user who never runs a graph with a
  sink never gets one):
  - **schema** and **role** both named `{org_slug}_{user_id}` — the org's unique
    slug and the user's uuid (dashes to underscores), readable and collision-
    free (org slugs are unique across the system);
  - the role is the schema's **owner** and holds no other grant — no rights on
    `public`, no rights on any service table, no `CREATE` anywhere else;
   - role provisioning **never puts a plaintext password in SQL**: the app
    computes a SCRAM-SHA-256 verifier and issues `CREATE ROLE … PASSWORD` with
     the verifier. The vault stores the password; transient app/runtime memory
     and libpq authentication necessarily handle it, never generated effect values.
- The credential lives at `secret/data/compute/{org_id}/{user_id}` (shape
  below). Vault policies: `typednotes-app` gets `create`, `delete` on the prefix
  (it provisions, and never reads back); a new **`lun`** service identity gets
  `read` — exactly how `liaison` reads `secret/data/thirdparty/`. On graduation
  to implementation, the identities move into [`connections.md`](connections.md)
  §4's table and `Fleet.lean`'s `vaultServiceIdentities`.

The app provisions with a dedicated connection string on `compute-db` only. That
identity **can run DDL** — which needs saying against the architecture's "no
serving identity can edit table structure" rule. The rule stands: it protects
*service* schemas, which live in `typednotes-db`, where the app still holds data
rights only. In `compute-db` the DDL *is* the product feature — creating user
schemas — and there is no table structure of ours to protect there.

**Enforcement — two layers, one truth.** The user's phrase "enforced by
effects" is exactly right and is two layers:

1. **Type layer.** The compiled `PostgreSQL` capability fixes non-secret
   host/port/database/role intent and permitted statement kinds/tables. Requests
   cannot substitute a connection. Queries are a structured AST with bound
   parameters and no `rawSql` escape hatch. `AuthorizedQuery` carries static
   rights, schema equality and a 63-byte table-identifier bound; `BoundCompute`
   proves target/role correspondence with the privately resolved credential.
2. **Runtime layer.** The canonical handler first checks independent live
   organization, actor-connection, cell and warrant documents, then resolves the
   credential only at `compute/{org_id}/{user_id}` and consumes the typed
   query/target witnesses. SQL explicitly qualifies the proved schema; it does
   not rely on a mutable search path. PostgreSQL's SCRAM role/schema ACLs enforce
   confinement independently. Actual responses are checked against byte ceilings.
   The model may see non-secret compute target metadata needed to compile its
   capability, but never a password or another actor's credential.

The type layer bounds what *this user's own code* can express; the runtime layer
is what keeps users apart. Neither is a sandbox by itself; both together mean a
sink can only write, as the right role, inside the right schema — and the
container remains the boundary for everything effects do not cover (as in
[`services/lun.md`](services/lun.md) §7).

**Credential shape** (written by the app, read by `lun`):

```jsonc
// secret/data/compute/{org_id}/{user_id}
{"kind": "postgres", "base_url": "<compute-db host:port>", "database": "compute",
 "schema": "{org_slug}_{user_id}", "token": "<role password>"}
```

`lun` resolves it per authorized compute operation — the same shape the session's
`(org_id, user_id)` binding selects. It never appears in a build request, a
graph program, a `lun.json`, or the repository.

### 4.2 `http`: call a service

The node's JSON result is POSTed to the URL the cell declares — an outbound
webhook: "POST the alert to the incidents service", "ping the healthcheck URL".
The URL may embed a token, as most webhook URLs do, so v1 needs no credential
machinery of its own; when the call *does* need an org connection's credential,
it is a `storage` sink's problem, solved the same way (§4.3).

- The egress honesty is exactly §3.1's: an anonymous POST from `lun`, not
  metered by liaison; what holds the line is that the app drives every
  recompute (so it knows each sink fired — the changed node, its outcome and
  its `Trace` come back in the update's answer) and the frequency bound is
  transitive (the table above).
- The URL gets the same SSRF guard as a `scheduled` source's (§3.1).

### 4.3 `storage`: the org's bucket, through the chokepoint

A `storage` sink writes to one of the org's existing storage connections
(`s3`, `azure`, `dropbox`, [`connections.md`](connections.md) §3.1).
That is a **credentialed** call, so the chokepoint holds here where §3.1 could
not: the credential stays in the vault, only `liaison` reads it, and the
function never sees it.

- S3/Azure functions declare `ObjectStore`; one bound grant pins the capability's
  bucket/container and exact configured key/prefix. Dropbox uses the native
  `Connector` effect with `files.create` and UTF-8 contents; it is not an
  ObjectStore bucket. The runtime sends URL-free `Body.connector` requests to
  `POST /v0/egress` with fresh named-operation warrants attached by the app
  (§2): `objects.write` or `files.create`, the named connection and structured
  selector, with the declared cost/budget. The app drives every
  update anyway, so refreshing the 300 s warrants is free, the same pattern as
  lode's credentials per message.
- The compiled capability contains resource intent, not a key. Organization,
  connection, cell and warrant ceilings intersect again at credential use in the
  broker. Shared credential-owner identity is separate from the execution actor.
- S3/Azure ObjectStore UTF-8 writes require empty options; binary writes/custom
  metadata and caller pagination cursors remain explicit refusals. Drive's current
  `files.create` creates an empty named file, not a contents-upload sink. Unsupported
  storage shapes never fall back to raw provider HTTP.
- Liaison's audit log carries the per-call record this time — the chokepoint
  earning its keep where a credential is actually at stake.

### 4.4 `ui`: rich output, rendered by the app

A `ui` sink is where the notebook shows its work: "show the invoice as a
table", "draw the price history as a chart". The node's result renders in the
notebook, and it re-renders on every recompute — a spreadsheet's output cell.

- **No raw HTML from the graph ever reaches the page — this is a hard rule,
  not a preference.** The graph's code is user code (the user's own repo, or
  what lode wrote from their prose); markup from it would be same-origin XSS
  against the app, one `tn_session` cookie away from account takeover. The
  function returns **JSON**, and the app renders it with a small set of vetted
  renderers: a table view, a chart spec, and markdown through a sanitizer. The
  cell's `{format}` names the renderer; an unknown format renders as the raw
  JSON.
- Rendering is client-side over typed models, never `dangerously_set_inner_html`
  of anything the graph produced.

### 4.5 `channel`: the result is a message

"Send the total back on the project's Slack channel", "reply with the
confirmation on WhatsApp". The cell binds to one of the project's messaging
interfaces ([`connections.md`](connections.md) §11); the node's result — the
message text — is sent by the **app**, through the same path it already sends
inbox replies: native liaison `messages.send` with a scoped warrant, the credential never leaving the
vault, the outbound message recorded in `channel_messages` with its sender.

- **The sending is app-side, not lun-side — deliberately.** The cell's node is
  a plain function (its row is `[]`: it composes the text); the app drives every
  recompute anyway, so when the update's answer reports the sink node changed,
  the app sends. No new effect for lun, no messaging credentials anywhere near
  a graph, and the chokepoint holds because the app's send *is* the existing
  liaison call.
- **Anti-spam is the graph's own equality:** the app sends only when the sink
  node's text **changed** — an unrelated recompute (another input moved, the
  source polled but the value held) re-renders the notebook without sending a
  word. A message sent is, by construction, a message whose content is new.
- The address is the channel's (the Slack channel, the WhatsApp number, the
  Signal number the project routed); replying to whoever triggered the run is
  an open question (§8) — the graph *knows* the peer (a `channel` source fed
  it), but a sink cell cannot name it yet.

## 5. Warrants and costs

| Call | Warrant (minted by the app, [`connections.md`](connections.md) §7 shape) | Budget |
|---|---|---|
| lode → repository | `repositories.read` plus supplementary `repositories.write`; independent `repositories.delete` scopes for removals; trusted publication branch/subtree | 0 |
| lode → model | `inference.generate`, named connection/model selector, trusted conversation/session tools | per-call `cost` (credits held by liaison) |
| lode/lun → repository read | `repositories.read`, named connection and immutable repository/file selector | 0 |
| lun → compute/graph secrets | `rows.select/insert/update/delete` or `secrets.read/write/describe/list`, actor-bound local connection and resource scopes | local operation cost 0; private runtime handlers |
| lun → storage sink write (§4.3) | S3/Azure `objects.write` or Dropbox `files.create`, exact key/path selector | declared per-call cost, attached to each update |
| app → channel sink send (§4.5) | `messages.send`, selected channel or sender/recipient selector | 0 — native messaging path |

Warrants expire within minutes (300 s); the app sends fresh ones with each lode
message (lode keeps them in memory only). Model spend is held and settled by
liaison per call against the org's credits — the same two caps (per-run,
per-org balance) as agent runs, and `lode`'s loop is additionally total
(`LODE_MAX_STEPS`).

What is **not** metered in v1: `lun` builds and calls (our compute, not a
provider call), source checks (§3). Both are open questions (§8) if graphs become
expensive.

## 6. Deployment — `typednotes-infra`

Two new containers in the fleet, declared in `Fleet.lean` like the others
([`typednotes-infra`](https://github.com/typednotes/typednotes-infra)):

| Container | Image | Env → wiring |
|---|---|---|
| `lode` | `ghcr.io/typednotes/lode:{release}` | `LODE_TOKEN` (secret), `LODE_LIAISON_URL` → `liaison`, `LODE_LUN_URL`/`LODE_LUN_TOKEN` → `lun`; no port published — only the app reaches it |
| `lun` | `ghcr.io/typednotes/lun:{release}` | `LUN_TOKEN`, `LUN_LIAISON_URL`, `LUN_ID_SALT` (build ids survive restarts), private vault login (`SECRETS_*`); compute target resolves from the actor credential |

Plus: **`compute-db`** (a `typednotes-`-prefixed Serverless SQL database — one
more billed instance, once, not per user), a **`compute` connection string for
the app** (DDL on that database only), the **`lun` vault service identity**, and
the app's new env: `LODE_URL`, `LODE_TOKEN`, `LUN_URL`, `LUN_TOKEN` (the app
submits builds and drives sessions itself, §2), `COMPUTE_DB_URL`. Neither lode nor
lun has migrations — a `postgresMigrations` history for `compute-db` applies one
bootstrap file that creates nothing but a marker schema, so `plan` shows the
database as managed from its first apply.

**Scale-to-zero is honest here, in both directions:**

- **No persistent volumes exist** for Serverless Containers, and lode and lun
  keep state on local disk. This is accepted, not worked around, because §2
  made every piece reconstructible: the notebook re-derives lode's session, the
  repository re-derives lun's build, the app's input log re-derives the
  session. What is *lost* on recycle is lode's in-flight run (the app shows it
  as failed; the user re-opens it) and lun's warm build cache (the next call
  rebuilds, minutes). An `objectStore`-backed workdir is the later upgrade if
  rebuild latency hurts (§8).
- **The source scheduler needs a process that is alive.** A scale-to-zero app
  has no ticker. v1 sets the app container's `minScale := 1` once any org has a
  scheduled source — the same trade `ledger`'s sweeper already makes. The clean
  fix is a Scaleway cron-triggered container, which `infra` does not yet
  declare; until it does, the floor is one always-on instance.

Named-operation and tool-ceiling proofs do not sandbox allowed shell commands or
Lakefiles. Separate organization/trust domains require process/container isolation;
a credential-less fleet-wide writer does not itself establish that boundary.
Apply separated vault ACLs for credential create/delete, policy projection writes
and runtime/broker reads. Deployment/isolation changes remain operator-owned.

## 7. The app's schema (migration `0004_computations.sql`)

The sketch below is what [`migrations/0004_computations.sql`](../migrations/0004_computations.sql)
implements, with these differences (§10 says why):

- `graph_cells` has a `name` (the identifier lode names the cell's function and
  input after), a `variant` column (the source's trigger or the sink's kind,
  none for a plain node), and `next_due_at` (the scheduler's due table); an
  endpoint's `token_hash` has a unique expression index;
- `graphs` also keeps the model (`model_connection_id`, `model_name`),
  `commit_sha`, the session's user (`session_user_id`), the last nodes and
  lun's reading of the structure;
- `source_checks` has `org_id` (for the hourly cap) and `latency_ms`;
- `compute_schemas` records which (org, user) got a schema and role (§4.1).

```sql
-- a graph (notebook) belongs to a project
graphs ( id, project_id → projects, slug, name, created_by → users,
         status ('editing' | 'implementing' | 'ready' | 'failed'),
         lode_session_id, lun_build_id, updated_at, unique (project_id, slug) )

-- the ordered cells: prose plus, once implemented, the derived contract
graph_cells ( id, graph_id → graphs, position,
              kind ('node' | 'source' | 'sink'),
              description text,
              config jsonb,          -- sources, by trigger:
                                    --   scheduled, watch: {url, schedule (cron), input}
                                    --   ui:      {input}
                                    --   secret:  {name} — never the value
                                    --   endpoint: {token_hash, input}
                                    --   channel: {channel_id, input}
                                    -- sinks, by kind:
                                    --   db:      {table}
                                    --   http:    {url}
                                    --   storage: {connection_id, path}
                                    --   ui:      {format}
                                    --   channel: {channel_id}
              impl jsonb )          -- once lode succeeded:
                                    -- {name, module, signature, effects, input}

-- every input the app ever fed a session (ui edits, scheduled checks, watch
-- changes, endpoint deliveries, channel messages): the log that re-registers
-- a lost lun session and that a `watch` source compares against. A secret
-- value never appears here or anywhere in this schema.
graph_inputs ( id, graph_id → graphs, input text, value jsonb,
               fed_by ('ui' | 'check' | 'endpoint' | 'channel'),
               cell_id → graph_cells?, at )

-- every session update the app drove, with what changed: the notebook's
-- history and the audit trail for sinks that fired inside the recompute
graph_updates ( id, graph_id → graphs, fed_by, cell_id → graph_cells?,
                changed jsonb, at )

-- one row per scheduled or watch check: the audit trail §3.1–3.2 promise
source_checks ( id, cell_id → graph_cells, started_at, ok, outcome, fed_at )

-- one row per endpoint delivery: the audit trail §3.5 promises
endpoint_calls ( id, cell_id → graph_cells, at, ok, status, fed_at )
```

The org's memberships already exist ([`services/core.md`](services/core.md) §4);
the notebook only adds the **invite flow** the schema anticipated — owners and
admins add a user to an org by email, creating the `memberships` row (TODO.md).

## 8. Open questions

- **Additional effects:** the current canonical set is `Trace`, `Error`, `HTTP`,
  `FileSystem`, `Connector`, `PostgreSQL`, `SecretStore`, `ObjectStore`.
  `Time` (a node that needs "now") remains absent;
  adding it means deciding what "now" means for a reactive node that re-runs.
- **Should an endpoint answer with what changed?** As specified (§3.5) the
  URL's holder reads the changed nodes — usable, but a leak if the URL was
  meant to be write-only. A per-cell "answer silently" flag is the knob; pick
  a default before the first endpoint is created.
- **Reply-to-peer for `channel` sinks (§4.5).** A `channel` source feeds the
  peer into the graph, but a `channel` sink can only send to the channel's own
  fixed address. "Reply to whoever asked" needs a config knob on the sink
  (`{channel_id, reply_to_input?}`) — small, but decide it with the first
  conversational graph, not after.
- **Credentialed sources** need liaison to accept a fetch with a warrant and no
  third-party connection — or a new "anonymous egress" call kind with its own
  budget and audit. Until then scheduled and watch sources are public pages
  only (§3.1).
- **Per-user DDL growth.** Roles and schemas accumulate; deleting a user's
  membership should drop (or archive) the schema. What "archive" means for user
  sink data is a product decision that should be made before the first schema
  is created, not after.
- **Metering builds and calls** (§5): free until graphs get expensive; if lun
  grows a bill (long builds, hot sessions), its calls become ledger events like
  any provider call.
- **`objectStore`-backed workdirs** for lode/lun (§6) if cold-start rebuilds
  hurt; `infra` declares buckets already.

## 9. Where the work lands

| Repo | Work |
|---|---|
| `typednotes` (this) | Implemented: notebook/UI/source contracts; clients and live three-document native authority provisioning/revocation (`0008`); actor SCRAM compute/graph-vault grants; writer conversation/publication refinements and org tool narrowing; scheduler/webhooks/native supported messaging; source recovery and adoption. Unsupported Signal history remains refused. |
| `typednotes-infra` | `lode` + `lun` containers; `compute-db` + its connection strings; `lun` vault identity; `LODE_TOKEN`/`LUN_TOKEN` secrets; app `minScale := 1` when sources ship |
| `lun` | Implemented: eight canonical bounded handlers, typed output/wiring/source contracts, immutable/narrowing sessions, native immutable fetch, actor/schema/graph confinement, pinned anonymous HTTP and scoped temporary files. |
| `lode` | Implemented: five native model protocols, bounded actual tool dispatch, retired-tool history recovery, native checkout/atomic publication and computation prompt. Web-fetch tool and Lean LSP remain open. |

## 10. The app, as implemented

`packages/api/src/computation.rs` (shared rules: cells, cron, `lun.json`,
widgets, the message to lode), `packages/api/src/server/{graphs,lode,lun,
scheduler,compute,members}.rs`, `packages/ui/src/{notebook,render,members}.rs`.
Decisions the design left open, or that the implementation had to make:

- **Naming.** Every cell has a `name`; lode is told to declare each cell's
  function under exactly that name and one graph named `main`, and each
  source's input as `input "{input}" T` (the input defaults to the name). The
  app maps lun's answers back to cells by these names. A notebook's Lean
  project lives at `typednotes/{graph_slug}` in the primary repository.
- **The implementing state** (`graphs.status = 'implementing'`) moves on when
  lode's run is over — noticed by the notebook's long poll or the scheduler's
  tick, which also refreshes lode's 300 s warrants while it runs: the app
  reads `lun.json` at lode's `workspace.remoteHead` through the repository
  connection, submits the build with a repo `read` warrant, and, once lun
  reports it ready, records each cell's implementation and registers the
  session. "Rebuild from the repository" does the same from the branch head.
- **Scheduled and watch functions** take the page URL as their argument
  (`String → Eff [HTTP] T`), so the URL the SSRF guard checked is the URL
  fetched. The guard resolves the host and refuses any private, loopback,
  link-local, CGNAT, multicast or documentation address — at save time and
   before each check. Lun's typed HTTP transport independently validates all DNS
   answers and pins the chosen numeric address, closing the re-resolution gap.
- **Caps.** `TYPEDNOTES_SOURCE_CHECKS_PER_HOUR` (default 120 per org; refused
  checks are audited but not counted) and `TYPEDNOTES_ENDPOINT_CALLS_PER_MINUTE`
  (default 60 per endpoint).
- **Endpoints answer with what changed** (§8's open question, decided as
  specified): `{"changed": [nodes]}`. A body whose JSON does not fit the
  input's base type (`Nat`, `Int`, `Float`, `String`, `Bool`, lists) is a
  `400`, recorded, session untouched.
- **Session binding.** A session is bound to the user who registered it
  (`session_user_id`); the scheduler, webhooks and channel messages act as
  that user, and `db` sinks write to that user's schema.
- **Re-registration** feeds the last recorded value of every input the build
  declares with `recoverInputs:true`; incompatible history after a type edit
  becomes an editable source error and blocks dependents. An update Lun answers
  `404` for re-registers first; invalid new input is refused without state changes.
- **Channel sources** are fed in the background, so a Slack or WhatsApp
  webhook is still answered within seconds.
- **Role names.** `{org_slug}_{user_id}` with dashes as underscores is up to
  77 characters, over Postgres's 63: a longer one keeps the slug's first 21
  characters and adds the org id's first 8 hex digits
  (`{slug21}_{org8}_{user32}`). Provisioning grants the role to the app's
  identity for the length of its transaction (to create the schema owned by
  it) and commits only once the vault holds the credential.
- **A cell's phases** (§1.2) are `graph_cells.writing` (set for the cells a
  run writes — all of them for "implement", one for a report or a failure —
  and cleared when the run's build is adopted), `issue` (why it is being
  rewritten) and `code_at`/`edited_at` (out-of-date code). The session runs
  `session_build_id`, the last adopted build, so inputs, checks and webhooks
  keep working while lode writes the next one as `lun_build_id`.
- **Automatic rewrites.** A failed build (lun's diagnostics, naming the cells
  they point at), a commit without a usable `lun.json`, or a node error on
  the recorded inputs sends the concerned cells back to lode by itself —
  once per distinct node error (`auto_issue`), and at most
  N times in a row until a member asks for a run again — N is set by the
  org's owners and admins on its settings page (`/orgs/{slug}/settings`, 0
  to 10, `0` disables; `orgs.auto_repairs`), else the deployment's
  `TYPEDNOTES_AUTO_REPAIRS` (default 2) — so a model that cannot fix it does not spend the
  org's credits in a loop. A node error can also be the data's fault (an
  unknown code in an input); the rewrite is told the arguments it failed on.
- **Vocabulary.** The services behind Typednotes (lode, lun, liaison,
  ledger, compute-db) are never named in the UI or in the messages it
  shows: they are "writing the code", "running", "the credential broker",
  "credits", "the notebook database". The first line of each message lode
   is sent is what the notebook's log shows of it, so it stays neutral, and
  lode's `lun_build`/`lun_call` tools show as "build"/"try". What the model
  itself writes in its steps is shown as written.
- **Members.** Adding an address nobody signed in with yet creates its
  `users` row: the first sign-in with that verified address links to it.
  Owners manage everyone, admins manage admins and members; an org keeps at
  least one owner.

Historical app-side verification on 2026-09-29 used Postgres 16 (both databases, SCRAM
enforced for the compute roles) with mock vault, liaison, lode and lun:
members; notebook and cells; the secret written to the vault and nowhere in
the database; lode's three warrants; build, adoption and session binding;
compute provisioning, the role logging in with the vault's password and
refused outside its schema; ui feeds, type refusals; endpoint delivery, bad
bodies, unknown and rotated tokens; the scheduler's tick feeding a check and a
watch seeing no change; a channel sink sending once and not again on an
unchanged recompute; a signed Slack message feeding a channel source, its
retry feeding nothing; re-registration from the input log; deleting a
notebook with its secrets; and a cell's life — every cell writing during
the first run, the steps and unpublished hunks that mention a cell, its
published definition, a member's report rewriting one cell while the old code
keeps answering, a node error sending its cell back once (not again for the
same error), a failed build rewritten twice and then failed, a description
edit marking the code out of date.

The 2026-09-30 release-preparation verification now includes the actual app →
compiled Lode → real broker → disposable local Git → compiled Lun pipeline:
native checkout/generation/tool execution/check, independent deletion denials,
atomic publication, adoption and caller-owned source constraints/recovery pass.
App-provisioned SCRAM compute and graph-vault effects, all four live ceilings,
shared external credential owners and monotonic tool/runtime refresh are executed,
not just mocked service responses. Supporting suites pass **99 API tests**,
**24 browser groups**, **655 real broker HTTP cases** and **69 compiled-runtime
cases**, plus the relevant Lean tests/proofs and executable builds. See
[`native-connectors.md`](native-connectors.md) for reproduction.

Provider/model replies are controlled local peers. Paid-provider conformance,
OAuth refresh and real-model implementation quality remain unmeasured; local
macOS checks do not establish Linux/container execution. Kernel guarantees
retain explicit build/container, approved library/FFI/syscall, database ACL,
vault/minting and transport/API trusted boundaries. Local compute/secret HMAC
authenticity trusts the authenticated app and stored projection; outbound HMAC
verification executes at the broker. Deployment ACLs/migrations and coordinated
release pins/publication remain parent/operator-owned.
