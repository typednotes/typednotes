# Typednotes v0.7.2

Publication-policy patch of v0.7.1, coordinated with Lode v0.4.2.

CI runs on main and PRs targeting main. Version-tag image publication requires
successful push-to-main CI for the exact tagged commit, with registry writes
scoped to the publication job. Main no longer publishes edge images. Stable
semver image tags retain latest; prereleases do not advance it.

Living README links point to GitHub main. The application runtime, connector
permissions, dependency pins, branding assets and migration history are unchanged.
Local gate/policy and branding checks passed; hosted CI and image publication
for this release remain pending the user's push.

Push main and wait for **CI**, then publish **v0.7.2** explicitly after Lode
v0.4.2's image is available. Wait for **Publish Docker image** before deploying.
The fleet's image selector resolves latest to a digest; schema adoption is a
separately reviewed release reference.
