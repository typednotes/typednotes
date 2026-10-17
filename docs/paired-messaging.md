# Signal and WhatsApp linked devices

Hosted QR pairing is the default in **Project settings → Interfaces** for Signal
and WhatsApp. Typednotes displays a short-lived code, the user scans it from the
phone's **Linked devices** screen, then selects **Use for this project**. Reloading
the page resumes an unfinished connection. Bound interfaces expose **Check
connection** and **Unlink device**. The previous Signal bridge and WhatsApp Cloud
API forms remain available as explicit advanced setups.

The protocol clients are native Rust libraries **loaded inside Liaison**:
Presage for Signal and `whatsapp-rust` for WhatsApp. No CLI subprocess or external
messaging daemon is required. The application remains write-only for vault
credentials. It creates a random encryption key; only the broker reads it and
owns device identities, sessions and protocol stores.

## Deployment

1. Deploy the matching Liaison changes and its native adapter libraries. The
   broker image builds both; see the sibling `liaison/native/README.md` for pins,
   build commands and licensing.
2. Set `LIAISON_PAIRED_DIR` on Liaison to a broker-owned **persistent encrypted
   volume**. WhatsApp's protocol store requires volume encryption; Signal's store
   additionally uses SQLCipher, and both recent-message inboxes are encrypted with
   their per-connection vault keys. An ephemeral serverless filesystem is not
   suitable. Routes must reach the broker owning the device; exclusive locks
   refuse a second worker on the same device, and replicas need a shared volume
   with working POSIX locks. A process supports up to 128 active devices.
3. Apply app migration `0016_paired_devices.sql` using the deployment migrator.
   Health checks require its per-conversation cursor table.
4. Keep the existing app/broker vault separation and `LIAISON_ROOT_KEY` setup.
   `GET /_health/paired` must report the selected provider as available before the
   app creates a connection. No Meta app secret or bridge token is needed for
   this pairing path.

## Authority and supported operations

New connections use the read-only preset: pairing does **not** grant sending.
Enable the organization connector effect/provider and explicitly grant
`messages.send` with the desired sender/recipient scope before sending from the
composer or a notebook. `messages.read` scopes use `[sender]` (all permitted
direct peers) or `[sender,peer]` (one conversation). Sends use `[sender,peer]`.
Exact conversation grants never become account-wide reads. The sender must equal
the account identity obtained during pairing. Signal account IDs and WhatsApp
direct LIDs are supported; Signal can also send to phone numbers present in its
synchronized contacts. Account IDs remain the unambiguous Signal read selectors.

Read calls accept an optional decimal-string `after` cursor and return at most
100 messages. The broker retains a bounded recent-delivery inbox of 2048 messages
and at most 8 MiB of text, not a complete historical archive. The app deduplicates
message IDs before advancing a separate durable cursor for each authorized
conversation selector. Scheduled sources and inbox refreshes use the same broker
path. Persisted project inbox records remain project data accessible to its org
members; connector permissions govern subsequent provider access.

This integration exposes direct text reads and sends. Group operations, media,
contact inventory, message edits/deletion and arbitrary history retrieval remain
unsupported. Existing Cloud API/bridge credentials still support their previous
send path; adding `messages.read` to their permissions does not provide a native
paired inbox or silently change their authentication.

Device creation/status/unlink are owner/admin-only application provisioning
operations. Their short-lived signed actions include the credential owner and
connection binding. They never appear in cell capability catalogs or presets,
and their warrants are never returned to the browser. Pairing does not widen the
organization, connection, cell or warrant ceilings. Messaging still consumes all
four at execution in Liaison, with the normal runtime capability checks in Lun.

Unlinking/removing a connection, deleting its organization or deleting its owner
closes live grants and stops/cleans the device before removing its vault key and
SQL records. Remote logout follows the provider library's semantics (WhatsApp is
best-effort); users can also remove Typednotes from their phone's Linked devices.
The cryptographic protocols, Rust libraries, C ABI, filesystem/process isolation,
volume encryption and remote provider remain trusted boundaries. Lean proves
authorization and signed-plan correspondence, not those external implementations.

## Verification boundary

Local tests cover parsing, storage encryption/restoration, exact peer filtering,
identity mismatch, expiry, filesystem locks/symlinks, private Lean witnesses and
the compiled broker → C → Rust path with offline protocol fixtures. Actual
phone scans, live sending/receiving and phone-side unlinking require a user-owned
account and are not established by those offline fixtures.

Executed locally for this change:

- 108 API tests and server/WASM checks passed.
- Lean broker build, proofs and tests passed.
- 701 real broker HTTP cases passed, with 167 supported operation/provider pairs
  and no catalog gaps.
- Real app/broker/SQL pairing flows passed for both providers. Real compiled Lun
  handlers reached the broker → C → Rust path and denied narrowed cell grants
  before credential reads.
- Five native storage/lifecycle tests and simultaneous loading/offline refusal
  of both real protocol libraries passed. Restart/unlink and pending-pairing
  cancellation are included.
- Focused browser checks passed for both providers, desktop/mobile QR display,
  provider-switch isolation, polling and connected/expired-code cleanup.

The broader project-note browser run timed out during its scheduling checks;
the focused QR run passed independently. The Linux image could not be exercised
locally because the container runtime was unavailable. CI includes native Linux
build/load tests; no deployment or live-account conformance is claimed here.
