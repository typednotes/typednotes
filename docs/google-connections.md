# Connect Google Calendar and Gmail

Google sign-in, Drive, Calendar and Gmail are separate grants even though they
use one OAuth client. Signing in with Google does not connect your mailbox or
calendar. Connections are shared within the selected Typednotes organization.

## Deployment setup

1. In the intended Google Cloud project, enable **Google Calendar API** and
   **Gmail API** under APIs & Services → Library.
2. Configure Google Auth Platform's branding, audience and data access (older
   console layouts call this the OAuth consent screen). For an External app in
   Testing, add every Google account that will test the integration as a test
   user. Workspace-only Internal clients work only for that Workspace audience.
3. Include the app's sign-in scopes `openid` and `email`, plus the scopes used by
   the connections you want:
   - Calendar: `https://www.googleapis.com/auth/calendar.readonly`.
   - Gmail: `https://www.googleapis.com/auth/gmail.readonly`.
4. Create a **Web application** OAuth client. Register the exact redirect URI:
   `https://app.typednotes.com/auth/google/callback`. For local development, also
   register `http://localhost:8080/auth/google/callback`. Use your actual app
   origin if it differs; the shared callback is `/auth/google/callback`, not
   `/auth/gmail/callback` or `/auth/google-calendar/callback`.
5. Set `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` on both the app and Liaison.
   Set the app's `PUBLIC_URL` to its browser-visible origin. Liaison needs the
   matching pair to refresh `google_oauth` tokens after the first access token
   expires. Configure the write-only app vault identity, the broker's credential
   read/refresh rights, `SECRETS_URL`, `SECRETS_PASSWORD`, `LIAISON_URL` and
   `LIAISON_ROOT_KEY`, and apply the full app migration history.

Google documents the exact capabilities in its
[Calendar scope reference](https://developers.google.com/workspace/calendar/api/auth)
and [Gmail scope reference](https://developers.google.com/workspace/gmail/api/auth/scopes).
`gmail.readonly` is restricted: public server-side use requires Google's applicable
verification/security assessment. External Testing refresh tokens for these
scopes typically expire after seven days; reconnect after expiry. See
[Google's OAuth guidance](https://developers.google.com/identity/protocols/oauth2/web-server).

## Connect and test

1. Open your organization → **Settings → Notebooks**. An owner/admin enables
   **Connector** under Effects, selects **Google Calendar** and/or **Gmail** under
   Connector providers, and clicks **Save permissions**. Inherited provider
   ceilings retain each connection's grants. Do not replace intentionally
   narrower organization grants with unrelated defaults.
2. Open **Settings → Connections**. Under Calendars select Google Calendar and
   click **Connect Google Calendar**, or under Webmail select Gmail and click
   **Connect Gmail**.
3. Choose the Google account, grant the requested read access and return to the
   app. Connect each service separately if you want both. The app requests
   `access_type=offline` and `prompt=consent`, then stores the tokens directly in
   the vault; they are never returned in the connection description.
4. On the new connection click **Test**. Calendar performs `calendars.list` and
   confirms a Calendar list response; Gmail performs `mailboxes.list` for `me`
   and confirms the label/mailbox list. Both calls go through Liaison's native
   adapter with a fresh, connection-bound warrant.
5. For a notebook cell, select this connection and narrow its operations and
   resources. Calendar event resources start with a calendar ID such as
   `primary`; Gmail resources start with the mailbox user `me`. Inventory grants
   (`calendars.list` at the account root, `mailboxes.list` at `me`) are separate
   from event/message grants. Keep them if you want the read-only Test button
   to continue working.

New connections use read-only application presets. OAuth scopes are another
ceiling: selecting write/send/delete in an app policy cannot turn these read-only
Google tokens into writable ones. No calendar/mail synchronization starts merely
because an account was connected.

## Troubleshooting

- **Connector access is disabled / live ceilings:** this is application policy,
  not a Google login failure. Enable Connector, the selected provider and the
  necessary scoped read operation in the organization and connection editors.
  Save explicitly; credentials or a notebook cannot bypass a disabled ceiling.
- **Connect button disabled:** check app health for Google client configuration
  and a successful vault login. The UI loads health separately so navigation
  remains responsive while the service is checked.
- **redirect_uri_mismatch:** the registered origin, port, scheme and callback
  path must exactly match the app's `PUBLIC_URL`-derived callback.
- **access_denied / app in testing:** add the chosen account as a test user, or
  use an account in the configured Internal audience. Workspace admin consent
  rules may impose an additional limit.
- **API disabled / SERVICE_DISABLED:** enable the relevant API in the OAuth
  client's project, not a different Cloud project.
- **invalid_grant or works initially then fails:** reconnect to obtain a new
  refresh token; confirm the same OAuth client pair is configured on Liaison.
- **Broker unreachable/refused:** verify the broker URL, warrant key, deployed
  native adapter contract and vault policy/projection namespaces described in
  [native-connectors.md](native-connectors.md).

For Microsoft Calendar, Outlook, CalDAV and JMAP/Fastmail, see
[connections.md §3.4](connections.md#34-calendar-webmail-and-workspace-setup).
