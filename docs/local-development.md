# Local app with local or remote services

The complete Dioxus frontend and API can run on your laptop while Liaison,
Secrets, Lode and Lun run remotely. Hot reload is local; connection calls and
notebook executions still use the real services and their permission checks.

## Start the app

Install the Dioxus 0.7 CLI and a Rust toolchain with the
`wasm32-unknown-unknown` target. From this checkout:

```sh
cp .env.local.example .env.local
# Edit .env.local with the backend addresses and credentials you intend to use.
bash scripts/dev.sh --check
bash scripts/dev.sh
```

Open `http://localhost:8080`. `.env.local` is ignored by Git. The launcher loads
it into the server process, binds the app to loopback and forwards extra arguments
to `dx serve`. `TYPEDNOTES_ENV_FILE=/absolute/path/to/private.env` selects another
file. `--check` validates the launch environment without contacting services or
printing credentials. For another port, set `PUBLIC_URL` to that origin and pass
`--port PORT`.

For an entirely local database, set `DATABASE_URL`, start PostgreSQL and run
`scripts/dev-db.sh` before the launcher. The server does not apply migrations.

## Connect to the deployed backend

Use private-network/VPN addresses or forwarded service ports. For example, if a
bastion can resolve the deployed service names (replace these illustrative names
and ports with your deployment's actual endpoints):

```sh
ssh -N \
  -L 15432:core-db:5432 \
  -L 15433:compute-db:5432 \
  -L 18200:secrets:8200 \
  -L 18081:liaison:8080 \
  -L 18082:lode:8080 \
  -L 18083:lun:8080 \
  user@bastion.example.com
```

Configure `.env.local` with:

- `DATABASE_URL`: the deployed **core/ledger database**, reached through the
  database tunnel using the app's normal data-only identity.
- `SECRETS_URL`, `SECRETS_USERNAME`, `SECRETS_PASSWORD`: the same write-only
  app identity and vault used by that broker.
- `LIAISON_URL`, `LIAISON_ROOT_KEY`: the app-visible broker address and its
  matching warrant signing key.
- `LODE_URL`, `LODE_TOKEN`, `LUN_URL`, `LUN_TOKEN`: the app-visible writer/runner
  addresses and their normal authenticated service tokens.
- `COMPUTE_DB_URL`: optional; the app-visible compute provisioning connection.

Keep these services on the same deployment contract versions documented in
[native-connectors.md](native-connectors.md). Use a dedicated development org,
project and repository branch when iterating: writes, model costs, messages and
permission edits in this configuration are real backend operations.

A fresh unrelated local core database cannot use the production broker by simply
changing `LIAISON_URL`: the broker checks organization credits in its own ledger,
and resolves credentials and live organization/run projections from its vault.
Use the shared backend database, or a fully matched staging/local service stack.
Membership checks, organization/user compute schemas, resource grants and all
four authority ceilings remain enforced.

### Addresses seen by remote workers

The laptop's `127.0.0.1` tunnels are not reachable from a remote Lun process.
Two optional settings separate laptop transport from worker transport:

```sh
LIAISON_URL='http://127.0.0.1:18081'
LIAISON_RUNTIME_URL='http://liaison:8080'
COMPUTE_DB_URL='postgres://provisioner:password@127.0.0.1:15433/compute'
COMPUTE_DB_RUNTIME_URL='postgres://compute-db:5432/compute'
```

`LIAISON_RUNTIME_URL` is placed in source/session/update envelopes sent to Lun;
it defaults to `LIAISON_URL`. `COMPUTE_DB_RUNTIME_URL` supplies only the host/port
for newly provisioned user credentials and non-secret compilation targets. It
must have the same database path as `COMPUTE_DB_URL`, no credentials, no query and
no fragment. It does not change the org/user-derived schema or generated role.
Both addresses must reach the same underlying services; the application cannot
prove that two DNS names/tunnels identify the same deployment.

Existing compute credentials retain the target originally provisioned in the
vault. Set worker addresses before provisioning a new development org/user.
Lode's own broker and runtime addresses are configured on the remote Lode
deployment (`LODE_LIAISON_URL`, `LODE_LUN_URL`, `LODE_LUN_TOKEN`).

## Sign in locally

`PUBLIC_URL` must name your local browser origin. Register
`http://localhost:8080/auth/google/callback` with the Google OAuth web client, or
use a GitHub OAuth app whose callback is
`http://localhost:8080/auth/github/callback`. The browser's localhost is correct
for these OAuth callbacks. Google connection refreshes require the matching
client pair on the broker; see [Google connection setup](google-connections.md).
If a provider cannot register both local and deployed callbacks, use a separate
development OAuth client with its matching broker deployment.

Local login uses a separate host-scoped cookie. It uses the same verified email
account in the shared backend; production browser cookies are not copied.

## Background work and local verification

The launcher defaults `TYPEDNOTES_BACKGROUND=0` so a local process does not claim
scheduled checks or follow deployments already managed by the remote app. Manual
API/UI actions still work. Set it to `1` to exercise scheduling deliberately.
Normal deployments default to enabled if the variable is absent.

The repeatable fixtures use disposable databases, fixture keys and local service
peers. They require no production login or paid provider:

```sh
cargo test -p api --features server --lib
cargo check -p web --features server
cargo check -p web --features web --target wasm32-unknown-unknown
uv run --with playwright scripts/test_notebook_ui.py --help
uv run --with playwright scripts/test_notebook_ui.py --output-parent /existing/scratch

# Real app and real native broker; requires a built sibling liaison binary.
cargo build -p web --features server
python3 scripts/test_native_connectors.py --temp-root /existing/scratch
```

The fixture scratch parent must already exist. Browser checks cover notebook
lifecycle, permission/TOML editing, denial recovery, same-document navigation,
mobile layout and idle-time slug validation. The broker fixture checks both
successful repository/model calls and independently denied authority before
credential reads. `--runtime` adds the compiled Lun effect path when the sibling
Lean workspace is available.

Verified locally on 2026-10-01: 105 API unit tests, server and wasm checks, 27
browser/app/database groups, 10 real app/broker/PostgreSQL groups, 8 compiled
app/runtime groups, and 35 offline release-gate cases. Cloud network access,
production OAuth credentials and paid upstream services are deployment-specific;
the fixtures do not substitute for testing those endpoints.
