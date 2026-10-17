# Typednotes v0.12.0 — hosted Signal and WhatsApp QR pairing

Coordinated with **Liaison v0.7.0**. This source release extends the v0.11.0
project-note/stateless runtime baseline; Lun/Lode/Linen contracts are unchanged.
Local commits and annotated release tags do not publish or deploy the services.

## Product and capability changes

- QR pairing is the default for Signal and WhatsApp in project Interfaces.
  Users scan from the phone's Linked devices screen, check status, resume
  unfinished setup, select the paired account for a project and unlink it.
- The broker loads native Rust protocol libraries in-process. Existing Cloud API
  and REST bridge setup remain explicit advanced choices.
- Paired connections support bounded direct-text `messages.read` and scoped
  `messages.send`. Sending remains an explicit independent grant; pairing does
  not widen organization, connection, project/cell or warrant authority.
- Owner/admin-only device control is signed separately from notebook effects.
  Device credentials remain broker-owned; the app writes but cannot read vault
  keys. Pairing responses are non-cacheable and expired/connected QR codes clear.
- Message ingestion deduplicates provider IDs before advancing durable,
  per-selector cursors. Connection and tenant deletion stop/clean paired devices
  before removing credentials and SQL records.

## Migration and rollout

Apply **`0016_paired_devices.sql`**, which adds recent-delivery cursor storage
under the existing channels' lifecycle. The app requires this table for schema
health. Existing migrations remain immutable; the app does not migrate itself.

1. Publish Liaison v0.7.0 and verify its exact-commit main CI/image publication.
2. Provision broker-owned **persistent encrypted storage** and configure
   `LIAISON_PAIRED_DIR` and the native adapters. Confirm `/_health/paired` reports
   availability. Ephemeral serverless storage is unsuitable for device keys.
3. Publish this app release, verify exact-commit CI/image publication, and update
   the deployment's migration source through 0016 before rolling out the app.
4. Verify linking, authorized direct-text messaging and phone-side unlinking with
   user-controlled accounts. A source tag does not apply cloud changes.

See [paired messaging](paired-messaging.md) and the sibling Liaison
`native/README.md` for storage, license, authority and operational boundaries.

## Local verification

- **108 API tests**, server/WASM checks, Lean broker tests/proofs and executable.
- **701 broker HTTP cases**, covering **167 operation/provider pairs** without
  catalog gaps, four ceilings, audit and hold settlement/release.
- Actual app/broker/SQL pairing flows for both providers; real compiled Lun
  handlers reaching broker → C → Rust, with narrowed-cell denials before reads.
- Five native storage/lifecycle tests; both real native libraries loaded together
  and exercised through strict refusal, offline state and teardown paths.
- Focused desktop/mobile browser checks for both providers, polling, provider
  switching and connected/expired-code cleanup.

The broader browser run timed out during scheduling checks. Live phone scans,
live send/receive/revocation and Linux container conformance remain unverified;
the container runtime was unavailable locally. These are not implied by the
offline native protocol fixtures or by Lean's authorization proofs.
