# Typednotes v0.8.0

## Changes

- Responsive settings navigation with client-side links and independent loading
  states, consistent page widths and improved form proportions.
- Labeled permission checkboxes, domain-policy radios, and a strict TOML editor
  that round-trips the same operation/resource policy and saves explicitly.
- Precise connection-ceiling diagnostics and recovery links. GitHub repository
  listing and Baseten testing are verified through the real native broker.
- Provider-loaded model menus for notebook generation, connection pricing and
  TypeSafe classification, using the existing bounded `models.list` operation.
- Idle-time slug/pricing queries with cancellation and stale-verdict protection.
- A private local development launcher, remote worker transport addresses and
  optional background scheduling; detailed Google Calendar/Gmail setup guides.
- Tagged image publication waits up to one hour for exact-commit main CI.
  Browser/branding checks are optional for PR/manual runs and required on main,
  where they run alongside the core checks in a separate job.

## Compatibility and deployment

This release changes the app only. No new sibling-service release, SQL migration
or connector operation is required. Retain the coordinated service contracts
described in [native-connectors.md](native-connectors.md), including the existing
Lode v0.4.2 deployment prepared for Typednotes v0.7.3.

New configuration is optional: `LIAISON_RUNTIME_URL`, `COMPUTE_DB_RUNTIME_URL` and
`TYPEDNOTES_BACKGROUND`. Existing deployments retain their current transport and
enabled scheduler defaults. See [local-development.md](local-development.md).

Organization/connection/cell/warrant ceilings remain enforced. An organization
with Connector disabled must explicitly enable it and save before repository or
AI connection calls can work. Empty writer-tool grants must also be configured
before generation; upgrading does not restore removed permissions.

Push `main` and `v0.8.0` together, or push main first and the tag afterward. The
publisher waits for main CI and publishes only on exact-commit success. Wait for
both **CI** (`check` and `extended`) and **Publish Docker image** before deployment.
The fleet's `latest` selector resolves to an immutable image digest during its
reviewed plan/apply; publishing the image does not itself apply the fleet.

## Local verification

- 105 API unit tests; server and wasm compilation checks.
- 27 real browser/app/PostgreSQL groups, with local writer/runtime/broker/vault
  peers, including model menus, TOML saves, same-document navigation and debouncing.
- 10 real app/native-broker/PostgreSQL groups and 8 compiled app/runtime groups,
  including independent four-ceiling and cross-user/schema denials.
- 35 offline release-gate cases, workflow policy checks and actionlint.

Production OAuth/network access and paid providers remain deployment-specific.
