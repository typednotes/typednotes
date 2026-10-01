# CI, release promotion and README link policy

## CI once, publication on version tags

Active code projects run their normal CI on pushes to `main` and pull requests
targeting `main`. Other branch pushes and version-tag pushes do not duplicate
that suite. Existing manual CI dispatch remains available where already supported.

Versioned publishers run only on `v*.*.*` tags. Their read-only verification job
consumes the actual tag checkout SHA, confirms the tag points to it, confirms
reachability from `origin/main`, and queries the correct CI workflow for its
latest exact-SHA **push-to-main** run. Only `completed/success` qualifies.
Missing, pending, failed, cancelled, PR or manual results cannot substitute.
The app publisher now polls missing/queued/running evidence instead of failing
immediately. `CI_WAIT_SECONDS` defaults to 3600 (maximum 7200), and
`CI_POLL_SECONDS` defaults to 15 (range 1–60). The verify job has a 70-minute
timeout. Every poll queries the latest run again; a newer failed attempt cannot
be hidden by an older success. Completed non-success and invalid/API evidence
fail immediately; a wait timeout produces no publish outputs.
API errors and malformed evidence fail closed. Secrets' manual crate retry and
Linen/Infra's release retries require an existing version tag and the same gate.

The publication job checks out the verified SHA. Registry/release write rights
are scoped to that job; the verification job has `contents:read` and `actions:read`.
This checks the prior test result without rebuilding/retesting all platforms on
tag pushes. Platform-specific and consumer tests remain in main CI. In particular,
Linen's release now attests its whole main workflow, including all native platform,
consumer, unsealable-host and HTTP/2 axes, rather than rerunning a narrower matrix.

Docker publishers keep semver image versions and shortened stable aliases.
Docker metadata v6 generates `latest` automatically for stable semver versions;
prereleases do not advance it. Main no longer publishes `edge`. The fleet uses
`latest` and resolves it to an immutable digest during plan/apply, so this does
not require changing its app image selector. A workflow publication is not a
cloud apply.

## Coverage and deliberate exceptions

The branch/PR filter is applied to the app, Linen, Ledger, Liaison, Lode, Lun,
Secrets, Infra, Typednotes-infra, Web-data, Lean-curl, Lean-pq, Lean-server and
Compiler. Compiler's automatic `compiler`-branch trigger is removed; PRs to main
remain tested. Infra's generated GitHub Plan workflow uses the same main/PR filter.
Versioned Docker publication is gated in app, Ledger, Liaison,
Lode, Lun and Secrets. Linen and Infra GitHub releases and Secrets crate releases
use the same promotion policy.

Home and Infra Pages deployments remain main-driven sites, not version-tag binary
publishers. Scheduled cleanup and manual-only cloud Apply/Destroy/live-test
triggers are retained. Typednotes-infra Plan keeps its existing read-only/live-main
conditions and credentials. Archived projects, vendored forks and repositories
without applicable workflows were not given invented release pipelines.

Required main-release checks, test matrices and platforms remain. The app's API,
server and release-policy checks run on every PR/manual invocation. Browser
compilation and branding run in a parallel **extended** job: always on main,
optionally on PRs with the `extended-ci` label, and on manual runs with the
**extended** checkbox. Label changes trigger a new PR run. Main success still
attests the full suite; an optional/manual-only result cannot release an image.
Release version, changelog, notes and artifact checks remain. Publishing an old tag uses the old
workflow stored in its commit; this policy applies to future committed tags.
If main and tag are pushed together, the app publisher waits for main CI; if it
fails or exceeds the timeout, fix/rerun CI and retry the publisher. Sibling
publishers retain their existing immediate-check behavior until coordinated.

## Readme references

Living README references to documentation/source use explicit GitHub `blob/main`
or `tree/main` URLs. Cross-project references point to the actual sibling
repository instead of local `../` paths. Inline labels, fragments and heading
anchors are preserved; local README section anchors remain local.

Dependency/build pins, immutable release contracts and historical release notes
keep their tag/commit references. External links, workflow badges, issue/release
pages and image/logo assets retain their appropriate URLs. A release snapshot's
README can therefore link to evolving documentation: use the immutable code/tag
when auditing a specific deployed version, not a living main page.

## Reproduction and trusted boundary

The app's `ci/require-main-ci.sh` and `ci/test-require-main-ci.sh` include bounded
waiting. Other publishers retain their shared immediate-check copies. Main CI runs the Bash behavior suite once
before publication can be attested. The app also checks local trigger/publisher
wiring and can audit sibling copy drift from a development workspace:

```sh
bash ci/test-require-main-ci.sh
/usr/bin/python3 ci/test-workflow-policy.py

# Development workspace only; CI does not require sibling checkouts.
python3 ci/test-workflow-policy.py --siblings /path/to/Typednotes

# Validate only the staged policy batch when other workspace edits are active.
python3 ci/test-workflow-policy.py --siblings /path/to/Typednotes --staged
```

The policy checker requires PyYAML; Ubuntu app CI installs `python3-yaml` and
uses its distro interpreter. Behavior tests create and clean disposable Git refs
and mock only the read-only Actions API; they never publish tags or artifacts.

Validation: **240 offline positive/negative gate cases**, **28 workflow files**
checked with actionlint, all **14 repository trigger/wiring/drift checks**, and
**238 distinct main-link destinations** checked locally. README fragments and
selected GitHub main/release targets were checked. No application runtime,
connector authority, dependency pin or logo asset changed in this policy work.
The app-local gate/policy checks and all five unchanged branding tests also
passed in a fresh Ubuntu 24.04 container using the documented apt dependencies.

The current app behavior suite adds missing-to-success and pending-to-success
transitions, timeout configuration and refusal of invalid polling parameters.
The gate trusts GitHub's authenticated Actions API, workflow identity, fetched Git
refs and runner environment. It is an operational promotion check, not a Lean
proof about external CI/registry infrastructure. The application's existing
kernel-checked effect/authority proofs remain unchanged.
