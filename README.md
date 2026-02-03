# Development

Depending on your selected options, your new workspace project contains a workspace member for each platform.
If you chose to develop with the router feature, each platform crate will have a `views` folder for your platform-specific views.
You are provided with a `ui` crate for shared UI and if you chose to use fullstack, you will have a `server` crate for your shared server functions.

## Setup

### Prerequisites

- [sops](https://github.com/getsops/sops) - for secrets management
- [yq](https://github.com/mikefarah/yq) - for YAML processing
- GPG key configured for decrypting `secrets.yaml`

### Environment

Generate `.env` from the encrypted `secrets.yaml`:

```bash
make env
```

This creates a `.env` file with database credentials and OAuth configuration for local development.

## Serving Your App

Navigate to the platform crate of your choice:
```bash
cd packages/web
```

and serve:

```bash
dx serve
```

## Building

`dx bundle --web --release --debug-symbols=false`
or
`dx bundle --fullstack --release --debug-symbols=false`

## Infrastructure

Deploy infrastructure to Scaleway:

```bash
make infra-up    # Generate tfvars, deploy, and update secrets.yaml
make infra-down  # Destroy infrastructure
make tfvars      # Generate infra/terraform.tfvars from secrets.yaml
```

`make infra-up` automatically:
1. Generates `infra/terraform.tfvars` from `secrets.yaml`
2. Runs `tofu apply`
3. Updates `secrets.yaml` with the database and container outputs

Run `make env` afterwards to regenerate your local `.env`.

## Migrations

To migrate the DB run:
`make migrate-run`

You need `sqlx` to be installed.

Run: `cargo install sqlx-cli --no-default-features --features postgres,rustls`
