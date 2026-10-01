# Typednotes

A server-side agent that acts on resources **on behalf of** users, with bounded,
auditable authority. System decisions: [`docs/architecture.md`](https://github.com/typednotes/typednotes/blob/main/docs/architecture.md);
per-service detail: [`docs/services/`](https://github.com/typednotes/typednotes/tree/main/docs/services/).

This repository is the **app**: a Dioxus 0.7 fullstack web app whose server is the minimal
`core` of the architecture — users, identities, orgs and memberships in Postgres
([`docs/services/core.md`](https://github.com/typednotes/typednotes/blob/main/docs/services/core.md) §4). Users sign in with GitHub or Google (the first sign-in creates
the account; the same verified email is the same account), create orgs and projects, and
connect code hosts (GitHub, GitLab), storage (S3-compatible buckets, Azure Blob Storage,
Dropbox, Google Drive) and AI accounts (Anthropic, Mistral, OpenAI, any OpenAI-compatible
endpoint), calendars (Google, Microsoft, CalDAV), webmail (Gmail, Outlook, JMAP/Fastmail)
and Notion workspaces to them. A project has a primary repository and messaging interfaces (Slack,
WhatsApp, Signal) whose messages land in its inbox. A project's computations are
notebooks of natural-language cells — graphs implemented by `lode` in the
repository and run by `lun`, fed by scheduled checks, widgets, webhooks and channel
messages ([`docs/computations.md`](https://github.com/typednotes/typednotes/blob/main/docs/computations.md)). Members are invited by email;
account, org and project settings each have their own page ([`docs/services/web.md`](https://github.com/typednotes/typednotes/blob/main/docs/services/web.md) §4b).
Credentials go into the vault; every provider call — "Test",
listing repositories, sending a message — goes through liaison. The contract with the other services
is [`docs/connections.md`](https://github.com/typednotes/typednotes/blob/main/docs/connections.md). Everything else in the architecture lives in
sibling services:

| Service | Repository | Deployed by |
|---|---|---|
| app (`web` + `core`) | this one | [`typednotes-infra`](https://github.com/typednotes/typednotes-infra/tree/main) |
| `ledger` — usage events, credits, holds | [`typednotes/ledger`](https://github.com/typednotes/ledger/tree/main) | [`typednotes-infra`](https://github.com/typednotes/typednotes-infra/tree/main) |
| `liaison` — the broker, sole egress chokepoint | [`typednotes/liaison`](https://github.com/typednotes/liaison/tree/main) | [`typednotes-infra`](https://github.com/typednotes/typednotes-infra/tree/main) |
| `secrets` — the vault | [`typednotes/secrets`](https://github.com/typednotes/secrets/tree/main) | [`typednotes-infra`](https://github.com/typednotes/typednotes-infra/tree/main) |
| `lode` — implements graph cells as Lean code in the repo | [`typednotes/lode`](https://github.com/typednotes/lode/tree/main) | [`typednotes-infra`](https://github.com/typednotes/typednotes-infra/tree/main) |
| `lun` — builds and runs the typed, reactive graphs | [`typednotes/lun`](https://github.com/typednotes/lun/tree/main) | [`typednotes-infra`](https://github.com/typednotes/typednotes-infra/tree/main) |

Sign-in is **interim**: GitHub and Google act as identity providers until `idp`
([`docs/services/idp.md`](https://github.com/typednotes/typednotes/blob/main/docs/services/idp.md)) exists. They are recorded as `identities` rows, so switching providers
later keeps every user id.

## Layout

```
packages/
  api/      shared types and server functions; `src/server/` is server-only:
              session + oauth   sign-in, cookies, the /auth/* routes
              db, connections   orgs, memberships, ledger's grant, connections
              projects          projects and their primary repository
              channels          interfaces, inbox, the /hooks/* webhooks
              vault, warrant, liaison   clients for secrets and liaison
  ui/       shared UI: sign-in, orgs, projects, connections, interfaces, and the dx components
  web/      the deployable fullstack app (routes, SSR, wasm client)
  desktop/, mobile/   the same UI for native targets (they need a server URL; not deployed)
migrations/           the core schema, NNNN_description.sql — the one source of truth
scripts/dev-db.sh     applies migrations/ (and sibling ledger/liaison sql/) to a local database
Dockerfile            `dx bundle` → the `web` server binary + `public/`
```

## Develop

For a hot-reloaded local app backed by deployed services, use the private
`.env.local` launcher and transport settings described in
[`docs/local-development.md`](https://github.com/typednotes/typednotes/blob/main/docs/local-development.md).
Google Calendar/Gmail setup has a step-by-step guide in
[`docs/google-connections.md`](https://github.com/typednotes/typednotes/blob/main/docs/google-connections.md).

The expanded AI provider catalog, environment-key setup and token-pricing UI are
documented in [`docs/ai-providers.md`](https://github.com/typednotes/typednotes/blob/main/docs/ai-providers.md). The shared shadowed
`⊢` logo and configurable SVG/PNG generator are described in
[`docs/branding.md`](https://github.com/typednotes/typednotes/blob/main/docs/branding.md).
Notebook declarations and permission setup are in
[`docs/notebook-ui.md`](https://github.com/typednotes/typednotes/blob/main/docs/notebook-ui.md); verified native contracts and
coordinated deployment requirements are in
[`docs/native-connectors.md`](https://github.com/typednotes/typednotes/blob/main/docs/native-connectors.md).
The Graph/Lode questions, scoped runtime trials and Lean LSP are answered in
[`docs/graph-and-lode.md`](https://github.com/typednotes/typednotes/blob/main/docs/graph-and-lode.md); CI fixes and executed checks
are recorded in [`docs/ci-verification.md`](https://github.com/typednotes/typednotes/blob/main/docs/ci-verification.md).
The current app release is Typednotes **v0.8.0**, compatible with the existing Lode
**v0.4.2** deployment. Release notes are in
[`docs/release-0.8.0.md`](https://github.com/typednotes/typednotes/blob/main/docs/release-0.8.0.md);
push order and exact CI gates are in [`docs/push-order.md`](https://github.com/typednotes/typednotes/blob/main/docs/push-order.md).
The shared CI/release and documentation-link conventions are in
[`docs/workflow-policy.md`](https://github.com/typednotes/typednotes/blob/main/docs/workflow-policy.md).

```sh
# a local Postgres, then the schema (the server never migrates itself)
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/typednotes
scripts/dev-db.sh          # also applies ../ledger/sql and ../liaison/sql when checked out

dx serve -p web            # http://localhost:8080
```

Every other variable is optional and turns one feature on ([`docs/connections.md`](https://github.com/typednotes/typednotes/blob/main/docs/connections.md) §8); the
status line on the home page lists what is missing.

| Variable | Enables |
|---|---|
| `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` | GitHub sign-in and connection (OAuth App, callback `http://localhost:8080/auth/github/callback`) |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | Google sign-in, Drive, Calendar and Gmail connections (callback `…/auth/google/callback`); enable Calendar/Gmail APIs and consent scopes, same client pair on liaison |
| `MICROSOFT_CLIENT_ID`, `MICROSOFT_CLIENT_SECRET` | Microsoft Calendar and Outlook connections (Entra Web app, personal + work/school accounts, callback `…/auth/microsoft/callback`); same pair on liaison with `microsoft_oauth` support |
| `GITLAB_CLIENT_ID`, `GITLAB_CLIENT_SECRET` | GitLab connection (gitlab.com OAuth application, callback `…/auth/gitlab/callback`); liaison needs the same pair to refresh |
| `DROPBOX_CLIENT_ID`, `DROPBOX_CLIENT_SECRET` | Dropbox connection (scoped app, callback `…/auth/dropbox/callback`); liaison needs the same pair to refresh |
| `SLACK_CLIENT_ID`, `SLACK_CLIENT_SECRET` | installing the Slack app in a workspace (callback `…/auth/slack/callback`, token rotation off) |
| `SLACK_SIGNING_SECRET` | inbound Slack messages (Event Subscriptions request URL `…/hooks/slack`) |
| `WHATSAPP_APP_SECRET`, `WHATSAPP_VERIFY_TOKEN` | inbound WhatsApp messages (Meta webhook `…/hooks/whatsapp`, field `messages`) |
| `SECRETS_URL`, `SECRETS_PASSWORD` (`SECRETS_USERNAME`, default `typednotes-app`) | storing connections, in a `secrets-server` ≥ 1.3.0 that declares the `typednotes-app` identity (`SECRETS_SERVER_SERVICE_IDENTITIES`; `typednotes-infra` does). `/api/health` reports `vault: true` only once the app can log in |
| `LIAISON_URL`, `LIAISON_ROOT_KEY` | native provider calls through liaison ≥ 0.6.0 (same root key and independent policy projections) |
| `PUBLIC_URL` | overrides the origin used in OAuth redirect URIs (default: the request's forwarded host) |
| `TYPEDNOTES_WELCOME_CREDITS` | credits granted to each new org (default 1000, `0` disables) |
| `LODE_URL`, `LODE_TOKEN` | implementing notebooks (lode ≥ 0.3.0) |
| `LUN_URL`, `LUN_TOKEN` | building and running them (lun ≥ 0.3.0) |
| `COMPUTE_DB_URL` | `db` sinks: an identity that may create roles and schemas on compute-db |
| `TYPEDNOTES_SOURCE_CHECKS_PER_HOUR`, `TYPEDNOTES_ENDPOINT_CALLS_PER_MINUTE`, `TYPEDNOTES_MODEL_CALL_COST`, `TYPEDNOTES_STORAGE_WRITE_COST`, `TYPEDNOTES_AUTO_REPAIRS` | notebook caps and costs (defaults 120, 60, 10, 1, 2 — [`docs/connections.md`](https://github.com/typednotes/typednotes/blob/main/docs/connections.md) §8) |

`cargo test -p api --features server` runs the unit tests (validation, sessions, PKCE,
warrant encoding, credential shapes, ledger's grant SQL, webhook signatures and payloads,
cron, cells, `lun.json`, SCRAM verifiers, renderers);
`cargo check -p web --features server` and
`cargo check -p web --features web --target wasm32-unknown-unknown` check both halves.

Calendar, webmail and Notion connections live in **Organization settings → Connections**.
Apply the complete migration history through `0008_connector_authority.sql` first. Google/Microsoft connections request
read-only access. CalDAV uses a calendar URL and app password; JMAP uses a session URL and
mail-enabled API token; Notion uses an internal integration or personal access token.
Provider setup, Gmail verification requirements and broker compatibility are documented in
[`docs/connections.md` §3.4](https://github.com/typednotes/typednotes/blob/main/docs/connections.md#34-calendar-webmail-and-workspace-setup).

## Migrations

`migrations/*.sql` is plain SQL and the only copy. In production,
`typednotes-infra` reads these files from GitHub at the release tag and
applies them as a declared `postgresMigrations` history (infra's
[`docs/migrations.md`](https://github.com/typednotes/infra/blob/main/docs/migrations.md)): reviewed as a plan before it runs, ordered before the
ledger's history (infra reads the order from the ledger's
`references orgs(id)`), and before the app container rolls out. The app's own
database identity has data rights only.

To add a migration: add `migrations/NNNN_description.sql`, tag a release, then
in `typednotes-infra` bump the app's release version and add the file to its
history — one line. **Shipped migrations are append-only**: infra refuses a
history whose applied prefix changed.

The 0.6.0 release adds `0006_ai_providers.sql`, `0007_notebook_contracts.sql`
and `0008_connector_authority.sql`. Apply them in order before the new app,
with Liaison 0.6.0, Lode/Lun 0.3.0 and Linen 1.10.0. The deployment must grant
the write-only app the new policy/projection namespaces and the runner the
separate local-service read rights documented in
[`docs/native-connectors.md`](https://github.com/typednotes/typednotes/blob/main/docs/native-connectors.md#trusted-provisioning).
Release pins and tags prepared locally are not a deployed environment.

## Deploy

Push the commit to `main`; a version tag can be pushed while **CI** is running.
CI also runs for PRs targeting `main`, but does not rerun on version tags.
Tag the same commit `vX.Y.Z`; the branch and tag may be pushed together:
[`.github/workflows/docker-publish.yml`](https://github.com/typednotes/typednotes/blob/main/.github/workflows/docker-publish.yml) publishes
`ghcr.io/typednotes/typednotes:X.Y.Z`, the major/minor selector, and `latest`.
Image publication runs only for version tags; main pushes no longer build `edge`.
The publisher automatically checks that the tag matches its checkout, is reachable
from `main`, and has a successful latest push-to-main CI run for that exact SHA.
The publisher waits up to one hour for missing/queued/running exact-commit main
CI, then publishes only after success. Failed, cancelled, PR-only or manual-only
results cannot authorize publication. API errors or malformed evidence fail closed.
Ordinary PR/manual CI runs the API/server/release-policy checks; browser compilation
and branding are opt-in via the `extended-ci` PR label or the manual **extended**
checkbox. Main always runs the full suite in parallel jobs, so release attestation
still includes both halves.
Only the verification job can read CI metadata; registry write access belongs
to the subsequent publisher, which builds the verified commit without rerunning
the test suite. Stable semver tags update `latest`; prereleases do not.
Wait for **Publish Docker image** on the tag before deploying. The fleet tracks
the app image through `latest`, resolved to an immutable digest, and adopts SQL
history through a separately reviewed release tag. Update that schema reference
when adopting migrations, then review the plan and apply in `typednotes-infra`.
The fleet's configured settings and additional notebook-service requirements are
documented in its [`Fleet.lean`](https://github.com/typednotes/typednotes-infra/blob/main/Fleet.lean) and
[README](https://github.com/typednotes/typednotes-infra/blob/main/README.md).
