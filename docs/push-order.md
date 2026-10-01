# Push order and CI gates

## Current app release: v0.8.0

The settings/model-menu/CI-waiting work is an app-only release. No additional
sibling service tag or SQL migration is needed; retain the existing coordinated
services (including Lode v0.4.2). See [release-0.8.0.md](release-0.8.0.md).

From the app repository, the branch and intended tag can now be pushed together:

```sh
git push origin main v0.8.0
```

The publisher waits up to one hour for exact-commit main CI. Wait for **CI**'s
`check` and `extended` jobs and **Publish Docker image** before deploying the
new image. A failed test run still blocks publication. Avoid `--tags`, which
would also publish unrelated local tags.

## Current organization workflow split

For future app commits, **CI** runs on pushes to `main` and PRs targeting `main`;
it does not run on tag pushes. **Publish Docker image** runs on `v*.*.*` tag
pushes only, publishing the release version, major/minor selector and `latest`.
There is no app image build or `edge` update on main pushes. Manual CI dispatch
remains available. This same main/PR/tag split applies to the active sub-projects'
CI and versioned publishers. Main-driven Pages sites and manual-only cloud
Apply/Destroy/live-test workflows retain their existing deployment semantics.

App gate: push main and the intended version tag → wait for successful **CI** on
that exact SHA and **Publish Docker image** → deploy. Publication
automatically verifies that the latest push-to-main CI run for the actual
tag checkout SHA completed successfully, and verifies reachability from `main`.
Missing/pending main CI is polled; failed/invalid evidence refuses publication.
The fleet's `latest` selector remains supported. See
[`workflow-policy.md`](workflow-policy.md) for coverage, tests and link policy.

## Earlier coordinated service batches

The corrected release pair was **Lode v0.4.1 / Typednotes v0.7.1**. The historical
batch below used v0.4.0/v0.7.0; those already-published tags stay unchanged.
The subsequent prepared pair was **Lode v0.4.2 / Typednotes v0.7.3**. Push main first
and wait for its exact-commit CI before publishing either local tag; wait for
Lode's tag image before publishing the app tag.

The earlier service batch committed changes in seven repositories and prepared
deployment tags for the new capabilities: **Lode v0.4.0** and **Typednotes v0.7.0**.
The other five repositories received branch commits without new deployment tags.
Lun v0.3.0, Liaison v0.6.0 and Linen v1.10.0 remain the published runtime/SDK
dependencies; fixing their workflow/test code does not require replacing them.
The current workflow/documentation batch prepares app/Lode patch tags to publish
the new gated image workflows; the other repositories need main commits only.

## 1. Independent native clients and documentation

Push `main` in **web-data**, **lean-curl**, and **typednotes-compiler**. These
three can be pushed in parallel and do not block the service SDK graph.

Before considering each repository finished, wait for:

- Web-data: **Lean Action CI**, both Ubuntu 24.04 and macOS jobs, including the
  executed native DuckDB success/error smoke.
- Lean-curl: **Lean Action CI**, both OS jobs and offline native transfer tests.
- Compiler: **CI**, both `build` and `docs`; the latter includes artifact checks
  and Pages deployment.

Lean-curl and Compiler were fast-forwarded to current upstream before applying
the fixes, so the prepared commits retain upstream work. Web-data also has its
previously prepared v0.1.3 and logo commits ahead of remote main; a main push
includes them. There is no new Web-data deployment tag in this batch.

## 2. Runtime and broker CI fixes

Push `main` in **lun** and **liaison**. They can be pushed in parallel: their
new workflows consume already-published peer refs, not unpublished commits from
this batch. Wait for both before advancing to Lode:

- Lun: **Lean Action CI** — `build` and `e2e`, including all typed/effect refusal
  checks, diamond emissions and temporary-file regressions. Its E2E job has a
  45-minute budget.
- Liaison: **Lean Action CI** — `build` and `native-contracts`.

These main pushes test changes but no longer rebuild `edge`. Existing release
version tags and `latest` are changed only by publication events. Historical failed
runs on v0.3.0/v0.6.0 do not become green just because main was repaired: check
the runs for the newly pushed main commits.

## 3. Lode branch, then deployment tag

Push **lode main**, and wait for:

- **Lean Action CI**: `build` (proof/unit/LSP/repository fixtures) and `lun`
  (integration with the pinned real runner).

After the branch workflow passes, publish a new version tag at that exact commit
and wait for **Publish Docker image** before pushing the dependent app. The
publisher attests main CI instead of repeating it on the tag. Lode v0.4.1 is the
corrected existing release; do not move an already-published tag to new code.

## 4. Earlier app branch, then deployment tag

Push **typednotes main**, and wait for:

- **CI**: the then-current `check` job (API/permission tests, server/wasm checks, logo contracts).

Then push the intended release tag after confirming that its commit SHA matches
that successful main CI run. Wait for **Publish Docker image** on the tag before
applying a deployment. CI is not repeated on the tag. Use a new version tag for
future commits; the corrected existing release is `v0.7.1`.

## Earlier batch commands

In each repository, push the branch first:

```sh
git push origin main
```

After the relevant branch gate, use these explicit tag pushes in their respective
repositories, in Lode → app order:

```sh
# Lode repository, after its main CI passes:
git push origin v0.4.2

# App repository, after Lode's image release and the app's main CI pass:
git push origin v0.7.3
```

Use the Actions page or `gh run list --repo typednotes/REPO` to identify the
new commit/ref, then `gh run watch RUN_ID --repo typednotes/REPO --exit-status`.
A green image build does not substitute for a green test workflow: these run
independently. Avoid `--tags`, which could publish other locally prepared tags.

## Deployment after the push gates

The existing Typednotes-infra fleet selects `latest` for the app image and
resolves selectors to immutable image digests during plan/apply. Publishing a
tag updates the registry; it is not itself a cloud apply. Run the reviewed
deployment only after both new service images are available, deploying the
updated app and Lode together. If a Lode service is managed outside this fleet,
update that service explicitly to v0.4.1 or the newly verified release; the current fleet does not declare it.

No schema migration was added after 0008. The fleet's existing v0.6.0 SQL-history
ref remains valid until a new migration is introduced. Replace legacy writer
sessions without a launch execution ceiling. Explicitly narrowed tool lists do
not automatically regain the new `lsp` permission.

This workflow/documentation batch does not apply a deployment or change the fleet.
