# Users and connections — the cross-service contract

**Status:** implemented v0 · **Last updated:** 2026-09-24

This document is the single contract that the app (`core` + `web`), `secrets`, `liaison`,
`ledger` and `typednotes-infra` implement for:

1. **user creation** — sign in with GitHub or Google (an interim issuer until `idp` exists);
2. **connections** — GitHub, Google Drive, an S3-compatible bucket, and an AI account
   (Mistral, OpenAI, Anthropic, or any OpenAI-compatible endpoint) with an API token.

It follows [`architecture.md`](architecture.md): the app writes third-party credentials into
`secrets`, **never reads them back**, and never returns them to the client; only `liaison`
reads them, behind a warrant check. Anything here that disagrees with a service's own code is
a bug in one of the two.

---

## 1. Flow

```
browser ──▶ app ──(OAuth code + PKCE)──▶ GitHub / Google            sign-in, connect
             │
             ├──▶ Postgres: users, identities, sessions, orgs, memberships, connections
             ├──▶ Postgres: credit_ledger welcome grant                       (ledger)
             ├──▶ secrets:  write  secret/data/thirdparty/{provider}/{user}/{conn}
             │
             └──▶ liaison: POST /v0/egress  (warrant minted by the app)       "Test"
                     ├──▶ ledger tables: hold, settle                         (ledger)
                     ├──▶ secrets: read credential (refresh Google, write back)
                     ├──▶ provider API (bearer / header / SigV4)
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
  issuer whose verified email matches an existing user links a new `identities` row to that
  user.
- Sessions: 32 random bytes, base64url, in the cookie `tn_session`
  (`HttpOnly; Secure; SameSite=Lax; Path=/`), stored as `sha256` in `sessions`. 30 days.
- The org creator becomes its `owner` (`memberships`). A user only sees orgs they belong to.

Callback URLs to register with the providers (`PUBLIC_URL` is the app's origin):

- GitHub OAuth App: `{PUBLIC_URL}/auth/github/callback`
- Google OAuth client (web): `{PUBLIC_URL}/auth/google/callback`

The same OAuth clients serve sign-in and connections; `oauth_flows.purpose` tells them apart.

## 3. Providers and credentials

A connection belongs to an **org** and to the **user** who created it. Its id is a UUID.

### 3.1 Provider ids

These strings are used, identically, as `connections.provider`, as the vault path segment,
and as the warrant `capability` provider.

| Provider id | How it is connected | `base_url` | Credential kind |
|---|---|---|---|
| `github` | OAuth (scopes `read:user repo`) | `https://api.github.com` | `bearer` |
| `gdrive` | OAuth (scope `https://www.googleapis.com/auth/drive`, `access_type=offline`, `prompt=consent`) | `https://www.googleapis.com` | `google_oauth` |
| `s3` | form: endpoint, region, bucket, access key id, secret | `{endpoint}/{bucket}` (path-style) | `s3` |
| `mistral` | form: API key | `https://api.mistral.ai/v1` | `bearer` |
| `openai` | form: API key | `https://api.openai.com/v1` | `bearer` |
| `anthropic` | form: API key | `https://api.anthropic.com/v1` | `header` (`x-api-key`, plus `anthropic-version: 2023-06-01`) |
| `openai-compatible` | form: base URL (https) + API key | as entered, no trailing `/` | `bearer` |

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

// s3 — AWS Signature Version 4, service "s3"
{"kind": "s3", "base_url": "https://s3.fr-par.scw.cloud/my-bucket", "region": "fr-par",
 "access_key_id": "SCW…", "secret_access_key": "…"}
```

`expires_at` is Unix seconds. A credential without a known `kind` is refused.

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
only to write back a refreshed Google token. Both log in with `userpass` and log in again
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
- Google: when `expires_at - 60 ≤ now`, liaison exchanges `refresh_token` at
  `https://oauth2.googleapis.com/token` with `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET`,
  uses the new token, and writes the updated credential back (best-effort).

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

For "Test connection" the app mints a warrant exactly as `liaison`'s `Tag.lean` verifies it:
`s₀ = HMAC(rootKey, lp(id) ‖ lp(orgId))`, `sᵢ = HMAC(sᵢ₋₁, caveatᵢ.toBytes)` in minting
order, where `lp` is a big-endian u64 length prefix. Caveats, in minting order:
`expiresAt(now + 300)`, `capability(provider, "read")`, `resource(connection_id)`,
`budget(0)`, `runId(uuid)`. JSON `caveats` are sent most-recent-first. `now` is Unix
seconds. `id`, `orgId` and `runId` are UUIDs (ledger's `credit_holds.run_id` is `uuid`).

| Provider | Test call |
|---|---|
| `github` | `GET https://api.github.com/user` |
| `gdrive` | `GET https://www.googleapis.com/drive/v3/about?fields=user` |
| `s3` | `GET {base_url}?list-type=2&max-keys=1` |
| AI providers | `GET {base_url}/models` |

## 8. Configuration

| Service | Variable | Notes |
|---|---|---|
| app | `DATABASE_URL` | unchanged |
| app | `PUBLIC_URL` | optional; otherwise derived from `X-Forwarded-Proto` + `Host` |
| app | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` | sign-in and the `github` connection |
| app, liaison | `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | sign-in and `gdrive`; liaison refreshes |
| app | `SECRETS_URL` | e.g. `https://secrets-server-….functions.fnc.fr-par.scw.cloud` |
| app, liaison | `SECRETS_USERNAME`, `SECRETS_PASSWORD` | `typednotes-app` / `liaison` |
| app | `LIAISON_URL`, `LIAISON_ROOT_KEY` | the same root key liaison verifies with |
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

## 10. Known gaps

- `liaison` still takes `now` from the caller; the app is the only caller.
- Deleting a vault user does not revoke tokens already issued to it, and `renew-self` has no
  maximum lifetime, so a holder can keep such a token alive by renewing it. Rotate by
  re-running the bootstrap script; a max TTL or revoke-by-owner in `secrets` is the real fix.
- Concurrent Google refreshes in liaison are not coalesced (last write wins in the vault).
- Linking by verified email trusts GitHub's and Google's verification.
- Warrants are minted only for "Test connection"; agent runs do not exist yet.
