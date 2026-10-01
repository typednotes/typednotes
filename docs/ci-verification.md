# CI fixes and verification — 2026-10-01

## Reported failures

- [Lun job 110114182132](https://github.com/typednotes/lun/actions/runs/36780449518/job/110114182132)
  failed because `test/e2e.sh` supplied a nonexistent sibling `../liaison` as the
  generated driver's SDK. A clean runner had only Lun and Linen checked out.
  The test now defaults to the SDK already fetched at Lun's locked
  `.lake/packages/liaison`, with an explicit override for unpublished local work.
- [Lode run 36780457777](https://github.com/typednotes/lode/actions/runs/36780457777)
  failed at the old expectation that input-only `lun_call` could execute Trace.
  Lun correctly refused missing authority. The regression now checks that this
  refusal is an actual tool error and verifies a pure graph separately. The new
  caller-owned bridge additionally verifies authorized effectful trials, rather
  than widening permissions to make the old test green.
- [Web-data run 36642326050](https://github.com/typednotes/web-data/actions/runs/36642326050)
  failed on unresolved `linen_duckdb_*` symbols. Linux now links Linen's sealed
  DuckDB DSO, where those shims live; it does not substitute upstream `libduckdb.so`.
  CI now covers Ubuntu 24.04/macOS and runs a linked success/error smoke executable.
- The older Lean-curl failed run's logs had expired. Its configuration contained
  a hardcoded ARM Linux link directory, and tests depended on public mutable
  HTTP services and swallowed errors. Architecture flags now come from pkg-config;
  Linux uses system clang for its system libcurl ABI. `lake test` runs asserted
  loopback GET/PUT/ten-transfer fixtures on Linux and macOS.
- [Compiler run 22465601448](https://github.com/typednotes/typednotes-compiler/actions/runs/22465601448)
  had a successful compiler build and failed docs job. Hosted main's moving
  doc-gen dependency required Lean 4.29.0-rc1 while the compiler pins 4.28.0.
  The root dependency and lock now use doc-gen4 v4.28.0, setup precedes resolution,
  the facet is `TypednotesCompiler:docs`, and generated artifacts are checked.
  See the compiler's `doc/ci.md` for the actual clean-build evidence.

## Regression coverage in workflows

- App: new CI runs API/permission tests, server/wasm checks and logo contracts;
  image publication was previously its only workflow.
- Lode: unit/proof tests, the actual Lean LSP dispatcher suite, repository/model
  fixtures, and real Lun integration. Jobs use Ubuntu 24.04 and explicit budgets.
- Lun: unit/proof and example builds, the full typed/runtime E2E suite, and
  descriptor-relative temporary-file regressions. E2E's budget is 45 minutes.
- Liaison: unit/proof tests plus real HMAC/PostgreSQL/native writer contract and
  catalog-drift fixtures, with pinned already-published peer revisions.
- Web-data and Lean-curl: Linux/macOS native build and executed offline fixtures.
- Compiler: warning-failing build, configured test runner, toolchain-matched docs,
  nonempty artifact checks, then Pages deployment.

Workflow validation with actionlint 1.7.12 passed for the app, Linen, Liaison,
Lode, Lun, Ledger, Web-data, Secrets, Infra, Typednotes-infra, Lean-curl, Lean-pq,
Lean-server and Compiler. Existing fleet edits and cloud-apply workflows were
not executed by this verification. Components whose current CI was already
successful were audited rather than given unrelated source changes.

## Executed local checks

- App: **101 API tests**, server/wasm checks, and **five logo tests** passed.
- Lun: **all 76 E2E checks** passed, including the original refusal checks and
  new shared-upstream diamond/repeated-emission checks. The complete local run
  took 30m09s; a 20-minute tool timeout was insufficient, not a runtime/proof stall.
- Coordinated Lean workspace: `LodeTest`, `LunTest`, Linen connector proof/tests
  and service executable builds passed.
- Lode/Lun integration: signature failure attribution, correction/publication,
  pure calls/graph execution and ungranted Trace denial passed.
- LSP: **63 actual dispatcher calls** passed against installed Lean 4.34,
  including hostile protocol/path/budget/abort and worker-cleanup cases.
- Real app → Lode → broker → local Git → Lun: the complete pipeline passed,
  including **seven new bounded Eff bridge groups**, eight compiled app/runtime
  groups and nine app/broker/SQL groups. Native DB, vault, files, HTTP, graph,
  Trace and HMAC connector successes/denials were executed, not only mocked APIs.
- Web-data: the full executable and native DuckDB success/error smoke passed on
  macOS arm64 and in a fresh **Ubuntu 24.04 arm64** container.
- Lean-curl: `lake test` passed on macOS arm64 and that Ubuntu container using
  native libcurl GET, streamed PUT and ten successful multi transfers.
- Compiler: warning-failing build, **42 test groups**, and actual docs generation
  passed. Artifact verification found **80 compiler pages** and **1,258 indexed
  compiler declarations** with valid source links.
- Actionlint and whitespace checks passed across the audited workflows/changes.

Local Linux verification does not pretend to be a new hosted x86_64 job. Updated
GitHub workflows still need the user's publication before they can run there;
rerunning an old commit retains its old failure. The verification did not publish
code/images, apply a cloud plan, or make billed model requests. Local deployment
tags are prepared separately; see [`push-order.md`](push-order.md).

## Deployment correspondence

Deploy Typednotes **v0.7.0** and Lode **v0.4.0** together for the new trusted execution envelope
and `lsp` tool. Lode now reuses the published Lun source revision
`291ae06d8f947140654091442ed28a7acfb0d037` for session projection/narrowing evidence.
Existing writers without a launch execution ceiling must be replaced, not granted
authority retroactively. Current explicitly narrowed tool lists do not gain `lsp`
automatically. Existing bounded runtime/broker policy and vault ACL requirements
remain unchanged.

The eight requested Graph/Lode answers are closed in
[`graph-and-lode.md`](graph-and-lode.md), with their exact semantics and proof/test
evidence. These CI fixes do not mark unrelated older optional sub-project backlog
features as implemented.
