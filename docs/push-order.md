# Push order and CI gates

This batch commits changes in seven repositories. Deployment tags are prepared
only for the new service capabilities: **Lode v0.4.0** and **Typednotes v0.7.0**.
The other five repositories get branch commits without new deployment tags.
Lun v0.3.0, Liaison v0.6.0 and Linen v1.10.0 remain the published runtime/SDK
dependencies; fixing their workflow/test code does not require replacing them.

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
  45-minute budget. Also wait for **Publish Docker image** from the main push.
- Liaison: **Lean Action CI** — `build` and `native-contracts`, plus **Publish
  Docker image** from the main push.

These main pushes rebuild `edge`; they do not replace the existing release
version tags or `latest` through a new version-tag event. Historical failed
runs on v0.3.0/v0.6.0 do not become green just because main was repaired: check
the runs for the newly pushed main commits.

## 3. Lode branch, then deployment tag

Push **lode main**, and wait for:

- **Lean Action CI**: `build` (proof/unit/LSP/repository fixtures) and `lun`
  (integration with the pinned real runner).
- **Publish Docker image** for the main commit.

After both workflows pass, push **only `v0.4.0`**. Wait again for the tag's
**Lean Action CI** and **Publish Docker image** to pass before pushing the app.
The tag publishes `ghcr.io/typednotes/lode:0.4.0` and updates `latest`.

## 4. App branch, then deployment tag

Push **typednotes main**, and wait for:

- **CI**: `check` (API/permission tests, server/wasm checks, logo contracts).
- **Publish Docker image** for the main commit.

Then push **only `v0.7.0`**. Wait for its **CI** and **Publish Docker image**
before applying a deployment. The tag publishes
`ghcr.io/typednotes/typednotes:0.7.0` and updates `latest`.

## Commands for the user

In each repository, push the branch first:

```sh
git push origin main
```

After the relevant branch gate, use these explicit tag pushes in their respective
repositories, in Lode → app order:

```sh
# Lode repository, after its main CI and image jobs pass:
git push origin v0.4.0

# App repository, after Lode's tag gates and the app's main gates pass:
git push origin v0.7.0
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
update that service explicitly to v0.4.0; the current fleet does not declare it.

No schema migration was added after 0008. The fleet's existing v0.6.0 SQL-history
ref remains valid until a new migration is introduced. Replace legacy writer
sessions without a launch execution ceiling. Explicitly narrowed tool lists do
not automatically regain the new `lsp` permission.

No Infra/fleet change or cloud apply is included in this commit batch.
