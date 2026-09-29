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

- [ ] Org members: owners and admins can add a user to the org by email (an `owner`/`admin` acts on `memberships`, which exist in the schema but have no UI) and remove one.
- [ ] `migrations/0004_computations.sql`: `graphs`, `graph_cells` (kind `node`/`source`/`sink`, description, config by trigger or kind, impl), `graph_inputs` (the input log that re-registers a lost lun session and that `watch` dedupes against), `graph_updates` (what changed, per update), `source_checks`, `endpoint_calls`.
- [ ] lode and lun HTTP clients (`packages/api/src/server/`): open/follow/steer a lode session with fresh warrants per message; submit a lun build from the published `lun.json`; register/update sessions with the `(org, user, graph)` binding and, for `storage` sinks, attached warrants.
- [ ] Warrants for lode (repo `write` on the project's branch, model per-call budget), lun (repo `read`) and `storage` sink writes (the `connections.md` §7 shape; channel sink sends reuse the existing messaging path).
- [ ] Notebook UI: the cells in order (description, config, the implementation lode produced, last outcome), "implement" and "steer" against lode, inputs and changed nodes against lun, with sources, sinks and the closure of the dependencies displayed. `ui` sink renderers: table, chart, markdown-through-a-sanitizer — never raw HTML from the graph.
- [ ] Source scheduler: cron due times, the tick that calls each due cell's function on lun and feeds the input; `watch` dedupe against `graph_inputs`; per-org rate cap; SSRF guard on URLs; audit in `source_checks`. Signal channel pulls ride the same tick.
- [ ] `ui` source cells: a widget per input (the widget follows from the input's JSON type lun reports), feeding the session, recorded in `graph_inputs`.
- [ ] `secret` cells: a write-only password field; the value goes to the vault at `secret/data/graph/{org_id}/{graph_id}/{name}` (app `create`/`delete`, `lun` `read`); functions declare `SecretStore`.
- [ ] `endpoint` cells: `POST /hooks/graphs/{token}` (token stored hashed, looked up by hash, rotatable; URL shown once); body feeds the input; the answer is the changed nodes; per-endpoint rate cap; audit in `endpoint_calls`.
- [ ] `channel` cells: sources feed new `channel_messages` rows (the message object) to the session; sinks send a changed node's text through the existing liaison send path and record the outbound message.
- [ ] Per-user compute: provision schema+role `{org_slug}_{user_id}` on `compute-db` (SCRAM verifier, never plaintext across Postgres), credential at `secret/data/compute/{org_id}/{user_id}` in the vault.
- [ ] Re-registration: when a lun session is gone (container recycled), rebuild it from `graph_inputs` before the next update.
- [ ] `LODE_URL`, `LODE_TOKEN`, `COMPUTE_DB_URL` in config and health (the status line on the home page).
