# Users and connections — the cross-service contract

**Status:** implemented v0.3 · **Last updated:** 2026-09-24

This document is the single contract that the app (`core` + `web`), `secrets`, `liaison`,
`ledger` and `typednotes-infra` implement for:

1. **user creation** — sign in with GitHub or Google (an interim issuer until `idp` exists);
2. **connections** — code hosts (GitHub, GitLab), storage (an S3-compatible bucket, an Azure
   Blob Storage container, Dropbox, Google Drive), an AI account (Anthropic, Mistral, OpenAI,
   or any OpenAI-compatible endpoint) with an API token, and messaging accounts (Slack,
   WhatsApp, Signal);
3. **projects** — a project's primary repository (§10) and its messaging interfaces and
   inbox (§11).

It follows [`architecture.md`](architecture.md): the app writes third-party credentials into
`secrets`, **never reads them back**, and never returns them to the client; only `liaison`
reads them, behind a warrant check. Anything here that disagrees with a service's own code is
a bug in one of the two.

---

## 1. Flow

```
browser ──▶ app ──(OAuth code + PKCE)──▶ GitHub / Google            sign-in, connect
             │  ──(OAuth code + PKCE)──▶ GitLab / Dropbox / Slack    connect
Slack, Meta ──▶ app /hooks/*  (signed webhooks)                      inbound messages
             │
             ├──▶ Postgres: users, identities, sessions, orgs, memberships, connections,
             │              projects, channels, channel_messages
             ├──▶ Postgres: credit_ledger welcome grant                       (ledger)
             ├──▶ secrets:  write  secret/data/thirdparty/{provider}/{user}/{conn}
             │
             └──▶ liaison: POST /v0/egress  (warrant minted by the app)   test, repos, send
                     ├──▶ ledger tables: hold, settle                         (ledger)
                     ├──▶ secrets: read credential (refresh OAuth, write back)
                     ├──▶ provider API (bearer / header / SigV4 / SAS)
                     └──▶ audit_log
```

## 2. Sign-in (app)

| Issuer (`identities.issuer`) | Subject | Scopes |
|---|---|---|
| `https://github.com` | GitHub numeric user id | `read:user user:email` |
| `https://accounts.google.com` | OIDC `sub` | `openid email profile` |

- Authorization code flow with PKCE (S256) and a single-use `state` stored server-side
  (`oauth_flows`, 10 minute lifetime).
- A user is created on first sign-in. The email must be **verified** (GitHub: the primary
  verified address from `/user/emails`; Google: `email_verified = true`). A sign-in with a new
  issuer links a new `identities` row to the existing user whose email is **any** of the
  addresses the issuer verified (GitHub: every verified address, primary first), compared
  case-insensitively — so Google then GitHub with the same address is one account. The
  lookup casts its parameter to `citext` (`users.email`): a `text` parameter compares
  case-sensitively, which, before 0.3, made a differently-cased address miss the lookup and
  fail on the unique index instead.
- Sessions: 32 random bytes, base64url, in the cookie `tn_session`
  (`HttpOnly; Secure; SameSite=Lax; Path=/`), stored as `sha256` in `sessions`. 30 days.
- The org creator becomes its `owner` (`memberships`). A user only sees orgs they belong to.

Callback URLs to register with the providers (`PUBLIC_URL` is the app's origin):

- GitHub OAuth App: `{PUBLIC_URL}/auth/github/callback`
- Google OAuth client (web): `{PUBLIC_URL}/auth/google/callback`
- GitLab application (gitlab.com): `{PUBLIC_URL}/auth/gitlab/callback`
- Dropbox app (scoped access): `{PUBLIC_URL}/auth/dropbox/callback`
- Slack app: `{PUBLIC_URL}/auth/slack/callback` (token rotation **off**: the bot token is
  stored as a plain `bearer`)

The same GitHub and Google clients serve sign-in and connections; `oauth_flows.purpose` tells
them apart. GitLab, Dropbox and Slack only connect accounts. A connect flow started from a
project page records it in `oauth_flows.return_to` (a `/orgs/{org}/projects/{project}` path
of the caller's org) and comes back there.

## 3. Providers and credentials

A connection belongs to an **org** and to the **user** who created it. Its id is a UUID.

### 3.1 Provider ids

These strings are used, identically, as `connections.provider`, as the vault path segment,
and as the warrant `capability` provider.

| Provider id | How it is connected | `base_url` | Credential kind |
|---|---|---|---|
| `github` | OAuth (scopes `read:user repo`) | `https://api.github.com` | `bearer` |
| `gitlab` | OAuth (scopes `read_user read_api read_repository write_repository`) | `https://gitlab.com/api/v4` | `gitlab_oauth` |
| `gdrive` | OAuth (scope `https://www.googleapis.com/auth/drive`, `access_type=offline`, `prompt=consent`) | `https://www.googleapis.com` | `google_oauth` |
| `dropbox` | OAuth (the app's scopes, `token_access_type=offline`) | `https://api.dropboxapi.com` | `dropbox_oauth` |
| `s3` | form: endpoint (presets: AWS `https://s3.{region}.amazonaws.com`, Cloudflare R2, Scaleway, other), region, bucket, access key id, secret | `{endpoint}/{bucket}` (path-style) | `s3` |
| `azure` | form: storage account, container, SAS token (or SAS URL) | `https://{account}.blob.core.windows.net/{container}` | `azure_sas` |
| `anthropic` | form: API key | `https://api.anthropic.com/v1` | `header` (`x-api-key`, plus `anthropic-version: 2023-06-01`) |
| `mistral` | form: API key | `https://api.mistral.ai/v1` | `bearer` |
| `openai` | form: API key | `https://api.openai.com/v1` | `bearer` |
| `openai-compatible` | form: base URL (https) + API key | as entered, no trailing `/` | `bearer` |
| `slack` | OAuth v2 app install (bot scopes `chat:write chat:write.public channels:read groups:read channels:history groups:history im:history`) | `https://slack.com/api` | `bearer` |
| `whatsapp` | form (project page): Cloud API phone number id + access token | `https://graph.facebook.com/v21.0` | `bearer` |
| `signal` | form (project page): signal-cli-rest-api bridge URL + registered number + bridge token | as entered | `bearer` |

`connections.external_id` holds the provider-side identity the app needs without the
credential: the Slack team id, the WhatsApp phone number id, the Signal number.

### 3.2 Vault path

```
secret/data/thirdparty/{provider}/{user_id}/{connection_id}
```

In `liaison`'s egress request this is `provider` + `call.account`, where
`call.account = "{user_id}/{connection_id}"`.

### 3.3 Credential JSON (written by the app, read by `liaison`)

Every value is a JSON **string** (numbers too), so no side parses floats. Every kind has
`kind` and `base_url`, and may carry `headers`: static headers `liaison` adds to each call.

```jsonc
// bearer — Authorization: Bearer {token}
{"kind": "bearer", "base_url": "https://api.github.com", "token": "gho_…"}

// header — {header}: {token}
{"kind": "header", "base_url": "https://api.anthropic.com/v1",
 "header": "x-api-key", "token": "sk-ant-…",
 "headers": {"anthropic-version": "2023-06-01"}}

// google_oauth — Authorization: Bearer {access_token}, refreshed by liaison
{"kind": "google_oauth", "base_url": "https://www.googleapis.com",
 "access_token": "ya29.…", "refresh_token": "1//…", "expires_at": "1790000000"}

// dropbox_oauth, gitlab_oauth — the same fields as google_oauth, refreshed at the
// issuer's own token endpoint (fixed in liaison, never read from the credential)
{"kind": "gitlab_oauth", "base_url": "https://gitlab.com/api/v4",
 "access_token": "…", "refresh_token": "…", "expires_at": "1790007200"}

// s3 — AWS Signature Version 4, service "s3"
{"kind": "s3", "base_url": "https://s3.us-east-1.amazonaws.com/my-bucket", "region": "us-east-1",
 "access_key_id": "AKIA…", "secret_access_key": "…"}

// azure_sas — the SAS appended to the query of every call
{"kind": "azure_sas", "base_url": "https://acme.blob.core.windows.net/notes",
 "sas": "sv=2022-11-02&sp=rl&se=…&sig=…", "headers": {"x-ms-version": "2021-12-02"}}
```

`expires_at` is Unix seconds. A credential without a known `kind` is refused. A `sas` may
contain only SAS parameters (`sv ss srt sp se st spr sig si sr sdd skoid sktid skt ske sks`),
including `sv` and `sig`.

## 4. `secrets`

- New endpoints, each requiring `sudo` on `auth/userpass/users/{username}`:
  - `POST /v1/auth/userpass/users/{username}` with `{"password": "…", "policies": ["…"]}`
    creates or replaces the user → `204`;
  - `GET` → `{"username": "…", "policies": [...]}` (never the hash);
  - `DELETE` → `204`, or `404` if absent.
  - The capability check runs before the body is read. `400` for a username outside 1–64
    characters of `[A-Za-z0-9_.-]` (or starting with `.`), a password under 12 characters,
    or an empty policy name. The bootstrap script requires 16 for the service passwords.
- Service identities, created once by `typednotes-infra/scripts/vault-bootstrap.sh`:

| User | Policy | Rules |
|---|---|---|
| `typednotes-app` | `typednotes-app` | `secret/data/thirdparty/` → `create`, `delete` |
| `liaison` | `liaison` | `secret/data/thirdparty/` → `read`, `create` |

The app can write and delete credentials but **cannot read them**. `liaison` needs `create`
only to write back a refreshed OAuth token. Both log in with `userpass` and log in again
before their token expires (1 h), which closes the static-token gap.

## 5. `liaison` — `POST /v0/egress` (0.3.0)

Additions to the 0.2.0 request, all backwards compatible except that credentials must now
have a `kind`:

```jsonc
"call": {
  "kind": "provider",
  "account": "{user_id}/{connection_id}",   // last segment must equal "resource"
  "method": "GET",
  "url": "https://api.github.com/user",     // must be base_url or under base_url + "/"
  "headers": {"accept": "application/json"},  // optional
  "body": "…"                               // optional, UTF-8 text
}
```

- `account` is two non-empty segments of `[A-Za-z0-9_-]`; its last segment must equal the
  request's `resource`, which the warrant's `resource` caveat binds. So a warrant for one
  connection cannot fetch another's credential.
- Caller headers named `authorization`, `proxy-authorization`, `x-api-key`, `host`,
  `content-length`, `transfer-encoding`, `connection`, `cookie`, `x-amz-*`, or any header the
  credential sets are refused, as are malformed names and values containing CR/LF.
- The URL must also parse as an `http(s)` URI without userinfo, fragment or dot segments, with
  the same scheme, host and port as `base_url`; `http` only if `base_url` is `http` (local
  testing). `method` is uppercase letters.
- S3 calls are signed over `host;x-amz-content-sha256;x-amz-date`; other headers are sent
  unsigned.
- A failure to settle or release the hold is audited and answered as `budget_unavailable`
  (`402`), even if the provider call already happened.
- New denials, each audited:

| Denial | HTTP | `error` |
|---|---|---|
| URL outside `base_url` | 403 | `url_denied` |
| forbidden caller header | 400 | `header_denied` |
| no credential / unusable credential / refresh failed | 502 | `credential_unavailable` |
| network failure reaching the provider | 502 | `upstream_failed` |

- Vault auth: `SECRETS_USERNAME` + `SECRETS_PASSWORD` (userpass login, re-login before
  expiry and once on `403`); `SECRETS_TOKEN` remains a fallback.
- OAuth refresh (0.4.0): when `expires_at - 60 ≤ now`, liaison exchanges `refresh_token` at
  the issuer's token endpoint with that issuer's client, uses the new token, and writes the
  updated credential back (best-effort):

| Kind | Token endpoint | Client | Refresh token |
|---|---|---|---|
| `google_oauth` | `https://oauth2.googleapis.com/token` | `GOOGLE_CLIENT_ID` / `_SECRET` | kept |
| `dropbox_oauth` | `https://api.dropboxapi.com/oauth2/token` | `DROPBOX_CLIENT_ID` / `_SECRET` | kept |
| `gitlab_oauth` | `https://gitlab.com/oauth/token` | `GITLAB_CLIENT_ID` / `_SECRET` | rotated every refresh |

- `azure_sas` (0.4.0): liaison appends the SAS to the call's query; a caller URL whose query
  uses any SAS parameter name (case-insensitively, percent-decoded) is `url_denied`.

## 6. `ledger` (0.3.0)

- `sql/0002_credit_ledger_idempotency.sql` adds `credit_ledger.idempotency_key text unique`.
- `Ledger/Sql/Reserve.lean` and liaison's `Budget.lean` cast every parameter
  (`$1::uuid`, `$3::bigint`, `returning id::text`). The driver sends parameters untyped, and
  without the casts Postgres refuses the reserve outright ("inconsistent types deduced for
  parameter $3"). That made every budgeted call before 0.3.0 fail as `budget_unavailable`.
- The grant statement, owned by `ledger` (`Ledger/Sql/Grant.lean`) and issued by the app on
  org creation:

```sql
insert into credit_ledger (org_id, delta, reason, idempotency_key)
values ($1::uuid, $2, 'grant', $3)
on conflict (idempotency_key) do nothing
```

- The welcome grant's key is `welcome:{org_id}`; the amount is the app's
  `TYPEDNOTES_WELCOME_CREDITS` (default `1000`, `0` disables).

## 7. Warrants minted by the app

For every provider call the app mints a warrant exactly as `liaison`'s `Tag.lean` verifies it:
`s₀ = HMAC(rootKey, lp(id) ‖ lp(orgId))`, `sᵢ = HMAC(sᵢ₋₁, caveatᵢ.toBytes)` in minting
order, where `lp` is a big-endian u64 length prefix. Caveats, in minting order:
`expiresAt(now + 300)`, `capability(provider, action)`, `resource(connection_id)`,
`budget(0)`, `runId(uuid)`. `action` is `read`, or `write` for sending a message. JSON `caveats` are sent most-recent-first. `now` is Unix
seconds. `id`, `orgId` and `runId` are UUIDs (ledger's `credit_holds.run_id` is `uuid`).

| Provider | Test call |
|---|---|
| `github` | `GET https://api.github.com/user` |
| `gitlab` | `GET {base_url}/user` |
| `gdrive` | `GET https://www.googleapis.com/drive/v3/about?fields=user` |
| `dropbox` | `POST {base_url}/2/users/get_current_account` (body `null`) |
| `s3` | `GET {base_url}?list-type=2&max-keys=1` |
| `azure` | `GET {base_url}?restype=container&comp=list&maxresults=1` |
| AI providers | `GET {base_url}/models` |
| `slack` | `GET {base_url}/auth.test` (a `200` with `"ok": false` is a failure) |
| `whatsapp` | `GET {base_url}/{phone_number_id}?fields=display_phone_number,verified_name` |
| `signal` | `GET {base_url}/v1/accounts` (the number must be listed) |

## 8. Configuration

| Service | Variable | Notes |
|---|---|---|
| app | `DATABASE_URL` | unchanged |
| app | `PUBLIC_URL` | optional; otherwise derived from `X-Forwarded-Proto` + `Host` |
| app | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` | sign-in and the `github` connection |
| app, liaison | `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | sign-in and `gdrive`; liaison refreshes |
| app, liaison | `GITLAB_CLIENT_ID`, `GITLAB_CLIENT_SECRET` | the `gitlab` connection; liaison refreshes |
| app, liaison | `DROPBOX_CLIENT_ID`, `DROPBOX_CLIENT_SECRET` | the `dropbox` connection; liaison refreshes |
| app | `SLACK_CLIENT_ID`, `SLACK_CLIENT_SECRET` | installing the Slack app (`slack` connection) |
| app | `SLACK_SIGNING_SECRET` | authenticates `POST /hooks/slack` |
| app | `WHATSAPP_APP_SECRET`, `WHATSAPP_VERIFY_TOKEN` | authenticates `/hooks/whatsapp` |
| app | `SECRETS_URL` | e.g. `https://secrets-server-….functions.fnc.fr-par.scw.cloud` |
| app, liaison | `SECRETS_USERNAME`, `SECRETS_PASSWORD` | `typednotes-app` / `liaison` |
| app | `LIAISON_URL`, `LIAISON_ROOT_KEY` | the same root key liaison verifies with (liaison ≥ 0.4.0) |
| app | `TYPEDNOTES_WELCOME_CREDITS` | default `1000` |

## 9. Verified end to end

On 2026-09-24, against Postgres 16 with every service's migrations applied in order, the local
`secrets-server` (bootstrapped by `vault-bootstrap.sh`), liaison 0.3.0 and the app. The provider
was a mock except where noted.

- An OAuth start stores the flow and redirects with PKCE. A bogus or replayed `state` is
  refused.
- Creating an org makes the creator its owner and writes the welcome grant (1000 credits).
- An `openai-compatible` connection is written to the vault. "Test" mints a warrant in Rust,
  liaison verifies it in Lean, holds and settles 0 credits, reads the credential as `liaison`,
  and calls the mock with the right `Authorization`. The audit row reads `ok`.
- An `s3` connection is SigV4-signed with the bucket's region. A `mistral` connection reached
  the real API over HTTPS (`401` for a fake key, as expected).
- The app's vault identity gets `403` reading a credential; liaison gets `403` deleting one.
  Removing a connection deletes its secret (`404` afterwards).
- A non-member gets `404` for another org and its connections. Sign-out clears the cookie and
  the session.

Not exercised: real GitHub and Google OAuth exchanges, and the Drive token refresh.

Again on 2026-09-24 for 0.3 (app) / 0.4.0 (liaison), locally, with the same stack:

- The linking lookup, as sqlx sends it (`text[]`), finds the Google-created user from a
  GitHub primary address in another case, and from a secondary verified address.
- Org and project slugs are reported available/taken/malformed while typing; a duplicate
  create is still `409`.
- Signed Slack and WhatsApp deliveries land in the routed project's inbox; retries are
  deduplicated; bad or stale signatures are `401`; bot messages and unrouted channels are
  dropped; both registration handshakes answer.
- A `signal` interface against a mock bridge on `http://localhost`: stored in the vault, tested,
  pulled into the inbox (once), and sent to (`write` warrant, JSON body) through liaison 0.4.0.
  Connecting the same number again is `409` and leaves no connection behind.
- An `azure_sas` credential against a mock container: liaison appends the SAS after the
  caller's query and sends the static `x-ms-version`; a wrong signature is refused upstream.

Not exercised: real GitLab, Dropbox and Slack OAuth exchanges, their refreshes, and the real
Slack, WhatsApp, Azure, GitHub/GitLab repository APIs.

## 10. Known gaps

- `liaison` still takes `now` from the caller; the app is the only caller.
- Deleting a vault user does not revoke tokens already issued to it, and `renew-self` has no
  maximum lifetime, so a holder can keep such a token alive by renewing it. Rotate by
  re-running the bootstrap script; a max TTL or revoke-by-owner in `secrets` is the real fix.
- Concurrent OAuth refreshes in liaison are not coalesced (last write wins in the vault).
  GitLab rotates its refresh token on every refresh, so for `gitlab` a lost race or a failed
  write-back means reconnecting.
- Dropbox's content endpoints (`content.dropboxapi.com`) are outside the `dropbox`
  connection's `base_url`; only the RPC API is reachable.
- A Slack app with token rotation on is refused at connect time (its bot tokens expire).
- Linking by verified email trusts GitHub's and Google's verification.
- Warrants are minted for tests, repository reads and messaging; agent runs do not exist
  yet, so nothing answers inbound messages.

## 10. Projects and their primary repository

A project belongs to an org (`projects`, unique `(org_id, slug)`); any member can create one,
its creator or an org admin can delete it. Its primary repository is chosen from what a `github`
or `gitlab` connection of the org can see, read through liaison:

| Provider | Listing | One repository |
|---|---|---|
| `github` | `GET /user/repos?per_page=100&sort=updated` | `GET /repos/{owner}/{name}` |
| `gitlab` | `GET /projects?membership=true&per_page=100&order_by=last_activity_at` | `GET /projects/{url-encoded path}` |

Setting it re-reads the repository, so its name, URL and default branch are the host's. The
project keeps `repo_connection_id` (set to null if that connection is removed — the name
stays, the access goes).

## 11. Interfaces and inbox

A **channel** binds one inbound address to one project, through a connection
(`channels`, unique `(provider, external_id, external_channel)` with nulls not distinct):

| Provider | Address (`external_id`, `external_channel`) | Inbound | Send (`write`) |
|---|---|---|---|
| `slack` | team id, channel id | Events API → `POST /hooks/slack` | `POST {base_url}/chat.postMessage` `{channel, text}` |
| `whatsapp` | phone number id, — | webhook → `POST /hooks/whatsapp` | `POST {base_url}/{phone_number_id}/messages` (text) |
| `signal` | number, — | `GET {base_url}/v1/receive/{number}`, pulled when the inbox loads | `POST {base_url}/v2/send` `{message, number, recipients}` |

- **Slack**: `X-Slack-Signature` = `v0=` hex HMAC-SHA256 of `v0:{timestamp}:{body}` with
  `SLACK_SIGNING_SECRET`, timestamp within 300 s. `url_verification` answers the challenge.
  Only people's plain messages are recorded (no `subtype`, no `bot_id`), keyed by `ts`.
  Deliveries no project routes are acknowledged and dropped.
- **WhatsApp**: `GET` with `hub.mode=subscribe` and `hub.verify_token = WHATSAPP_VERIFY_TOKEN`
  echoes `hub.challenge`. `POST` bodies are authenticated by `X-Hub-Signature-256` =
  `sha256=` hex HMAC-SHA256 of the body with `WHATSAPP_APP_SECRET`. Messages are routed by
  `metadata.phone_number_id` and keyed by their `wamid`; non-text messages are recorded as
  `[image]`, `[audio]`…
- **Signal** has no official bot API: a signal-cli-rest-api bridge (normal or native mode —
  json-rpc mode only offers a websocket) behind a reverse proxy that checks the bridge token as
  a bearer. Messages are keyed by `{source}:{timestamp}`.
- Messages (`channel_messages`) are unique per `(channel, direction, external_id)`, so
  retried webhooks and repeated pulls record nothing new. Outbound messages record who sent
  them.
