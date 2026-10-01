# Typednotes v0.7.3

Adds a small β symbol next to the app name in the header and browser title.
Server and browser compilation checks pass; notebook runtime code, permissions,
dependency pins, migrations and logo assets are unchanged from v0.7.2.

The coordinated deployment fix is Typednotes-infra v0.6.0, using Infra v0.22.1
to declare Lode/Lun, service authentication, scoped vault access and a separate
compute database. Publishing the app image alone does not configure these services.

Push app main and wait for CI before publishing v0.7.3. Complete the fleet's
documented secret setup and manual apply to enable notebook generation.
