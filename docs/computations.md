# Computations

**Status:** design · **Last updated:** 2026-09-29

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
 lun ──(storage sinks: warrants the app attaches)──────────▶ liaison ──▶ S3 / Azure / Dropbox / Drive
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
  `ObjectStore`. A `ui`, `endpoint` or `channel` source compiles to no function
  at all — it is an input with a trigger (§3) — and a `channel` sink's node is
  plain (`[]`): the sending is the app's, after the recompute (§4.5).

The graph program wires the functions together in linen's `Reactive` monad:
wiring a function to a value of the wrong type does not compile, the graph is
acyclic by construction, and a failing node's error stays with its node — its
dependents are skipped, everything else carries on. The notebook UI displays,
per cell, the sources, the sinks, and the dependencies (direct and closure)
that `lun`'s build answer reports for the graph.

### 1.2 What the user sees

The notebook page shows the cells in order, each with its description, the
implementation `lode` produced for it (function name, signature, effects), and —
once the graph is built — the node's last outcome. Inputs appear as `ui` cells
with widgets (§3.3); a change feeds the session and only what depends on it
runs again. The implementation log (lode's session) is followed in the same
page while it runs.

## 2. From description to running graph

Three steps, all driven by the app, all audit-attributed to a run:

1. **Implement — `lode`.** The app opens a lode session over the project's
   primary repository (a GitHub or GitLab connection of the org, through
   liaison, §5) with a message assembled from the cells, and mints three
   warrants: **repo `write`** (publish on the project's branch), **model**
   (the AI provider the org connected, per-call budget), **repo `read` for
   `lun`**. `lode` writes the modules, the `lun.json`, publishes **one commit on
   the shared branch** (never a force push), then `lun_build`s the published
   commit and fixes what lun reports per function and graph. A session is done
   when the published commit builds clean and the functions answer as intended.
   The user steers in natural language mid-run; the notebook re-implements any
   cell by a follow-up message.
2. **Build — `lun`.** The app submits the build (source: repo, branch, commit,
   project path; the functions and graphs from `lun.json`). Same request, same
   build id: a restarted lun re-derives it from the repository alone.
3. **Run — `lun` session.** The app registers the graph as a live session with
   the initial inputs and a **binding `(org_id, user_id, graph_id)`** — what
   lun's `PostgreSQL` and `SecretStore` handlers resolve their capabilities
   from (§3.4, §4.1) — plus, for the graph's `storage` sinks, fresh warrants
   attached to the update (§4.3). An update feeds only the inputs it names;
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
requests (§8). Two further bounds, both to be tested (tier 5): lun's `HTTP`
handler must refuse non-`http(s)` schemes, and the source scheduler must refuse
URLs that resolve to private address space (SSRF against the container
network's neighbours).

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

The widget follows from the input's JSON type, which lun's build answer
reports: a number gets a stepper, a string a text field, a boolean a toggle, a
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
  reads it per session as its own vault identity, exactly like the compute
  credential (§4.1).
- A function reaches it through the vetted `SecretStore` effect: the capability
  separates `describe` from `getValue` as distinct permissions, names are
  segments so a prefix can be scoped, and the value is linen's opaque
  `Secret.Value` — no `ToJson`, no rendering — so **it cannot land in a node
  output, a log line or a `lun.json` by accident.** lun's handler grants
  `getValue` on exactly the names the session's graph declares, nothing wider.
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

The project's messaging interfaces ([`connections.md`](connections.md) §11 —
Slack, WhatsApp, Signal) are already inbound addresses routed to the project;
a `channel` source cell binds one to the graph: "when a message arrives on the
project's WhatsApp number, that's the order."

- The app feeds the **message object** — `{text, peer, peer_name, at}` — as the
  input the cell declares; the prose says what to make of it, and `lode` wires
  the parsing node downstream ("the first number in the text is the amount").
- The delivery machinery is the inbox's, unchanged: the same signed webhooks
  (Slack, WhatsApp), the same dedupe — `channel_messages` is unique per
  `(channel, direction, external_id)`, so a retried webhook records nothing new
  and feeds nothing new — and Signal's pull model rides the scheduler tick
  rather than an inbox load. **The audit trail is `channel_messages` itself**,
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
| `storage` | `{connection_id, path}` | `ObjectStore` | the org's storage connection, **through liaison** (§4.3) |
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
  - the role's **password never crosses Postgres in the clear**: the app
    computes a SCRAM-SHA-256 verifier and issues `CREATE ROLE … PASSWORD` with
    the verifier, so the plaintext exists in exactly one place — the vault.
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

1. **Type layer.** A function without `PostgreSQL` in its effect row cannot
   touch any database at all — `lun` refuses the signature, and linen's
   capability-restricted effect, `Control.Monad.Effect.PostgreSQL`, carries no
   connection: **the effect names no host, no role, so a term of the
   function's type cannot name another user's database.** There is
   nothing to check because there is nothing to say. Queries are a structured
   AST, parameters always bound, no `rawSql` escape hatch — injection and
   "checked SQL disagreeing with sent SQL" are both unrepresentable.
2. **Runtime layer.** `lun`'s handler binds the effect to the session's user:
   it reads the credential from the vault as `lun` (path above), connects to
   `compute-db` **as the role `{org_slug}_{user_id}`**, and sets the schema in
   the search path. Postgres itself then refuses anything outside that schema.
   The compiled project never contains a connection target; it is not secret
   material withheld from lode's model — it is simply not in the code, because
   the code has nowhere to put it.

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

`lun` resolves it per session registration — the same shape the session's
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
(`s3`, `azure`, `dropbox`, `gdrive`, [`connections.md`](connections.md) §3.1).
That is a **credentialed** call, so the chokepoint holds here where §3.1 could
not: the credential stays in the vault, only `liaison` reads it, and the
function never sees it.

- The function declares `ObjectStore` in its row; `lun`'s handler relays the
  write to `POST /v0/egress` in liaison's wire format — the same call the app
  itself makes — with a warrant the app **attaches to the session update**
  (§2): `capability(s3|azure|dropbox|gdrive, write)`,
  `resource(connection_id)`, a small per-call budget. The app drives every
  update anyway, so refreshing the 300 s warrants is free, the same pattern as
  lode's credentials per message.
- The credential and the path are the only things per session: like
  `PostgreSQL`, the compiled project names no bucket and holds no key — the
  effect carries no connection, and the handler resolves `{connection_id, path}`
  from the session's config.
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
inbox replies: liaison with a `write` warrant, the credential never leaving the
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
| lode → repository read/write | `capability(github|gitlab, write)`, `resource(connection_id)` | 0 |
| lode → model | `capability(provider, read)`, `resource(model_connection_id)` | per-call `cost` (credits held by liaison) |
| lode/lun → repository read | the repo warrant, or lun's own (default: the source's) | 0 |
| lun → storage sink write (§4.3) | `capability(s3|azure|dropbox|gdrive, write)`, `resource(connection_id)` | small per-call, attached to each session update |
| app → channel sink send (§4.5) | `capability(slack|whatsapp|signal, write)`, `resource(connection_id)` | 0 — the existing messaging send path |

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
| `lun` | `ghcr.io/typednotes/lun:{release}` | `LUN_TOKEN`, `LUN_LIAISON_URL`, `LUN_COMPUTE_HOST` (compute-db's endpoint), `LUN_ID_SALT` (secret — build ids survive restarts), vault login for `lun` (`SECRETS_PASSWORD`) |

Plus: **`compute-db`** (a `typednotes-`-prefixed Serverless SQL database — one
more billed instance, once, not per user), a **`compute` connection string for
the app** (DDL on that database only), the **`lun` vault service identity**, and
the app's new env: `LODE_URL`, `LODE_TOKEN`, `COMPUTE_DB_URL`. Neither lode nor
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

`lode` runs one trust domain per org eventually (its `bash` tool runs what the
model asks); v1 runs one `lode` for the fleet and keeps it credential-less — it
holds only short-lived warrants — which is the same containment story as
[`services/agent.md`](services/agent.md) §2.

## 7. The app's schema (migration `0004_computations.sql`)

Sketch, the same conventions as [`services/core.md`](services/core.md) §4; the
SQL itself lands with the implementation:

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

- **Which effects may a node declare?** v1: `Trace`, `Error`, `HTTP`,
  `FileSystem`, `PostgreSQL`, `SecretStore`, `ObjectStore` — lun's vetted set
  plus the new ones. `Time` (a node that needs "now") is conspicuously absent;
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
| `typednotes` (this) | `0004` migration; notebook UI; lode + lun HTTP clients; warrant minting for them; the scheduler (cron due times, watch dedupe, Signal pulls) + rate caps; the `/hooks/graphs/{token}` route; channel sources fed from `channel_messages` and channel sinks sent through the existing path; `ui` sink renderers (no raw HTML); schema/role provisioning on `compute-db`; compute + graph-secret credentials into the vault; org invite flow |
| `typednotes-infra` | `lode` + `lun` containers; `compute-db` + its connection strings; `lun` vault identity; `LODE_TOKEN`/`LUN_TOKEN` secrets; app `minScale := 1` when sources ship |
| `lun` | vet the `PostgreSQL` and `SecretStore` effects + handlers bound per session to the vault-read credentials; relay `ObjectStore` writes through liaison with the attached warrants; refuse non-`http(s)` and private-range URLs in `HTTP`; add the compute and graph-secret credential kinds |
| `lode` | `fetch` tool; Lean LSP loop; the computation session prompt (the cell taxonomy → `lun.json` with sources/sinks) |
