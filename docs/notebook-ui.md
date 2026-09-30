# Notebook declarations and permission setup

## Cells

The notebook's cell name is its reference name. Enter input names in argument
order, use the `@name` picker, or refer to `@name` in the description. Explicit
prose references are appended to the declared arguments if missing. Ordinary
words and email addresses are not references. Unknown names, duplicate arguments,
self references, references to secret values, and dependency cycles are refused
before saving.

Renaming a cell updates its dependents' named arguments and whole `@name`
references in one database transaction. Dependent declarations become stale.
Deleting a referenced cell is refused: remove the dependent references first.
The running implementation is replaced when a new build is adopted.

Declaration numbers follow the saved declaration positions. Name and dependency
sorting only change presentation. Dependency order uses name, then cell ID, to
break ready-node ties. Moving a cell in declaration order persists its new
position. Keyed cell views preserve editing state and unsent source values while
sorting. The dependency graph shows input-to-dependent arrows; its selectable
nodes support pointer, Enter and Space activation.

Each cell has prose, an optional Lean output constraint, and the adopted code
and signature. Argument types come from upstream cells. Function results must
be `Eff effs T`; the optional field constrains `T`. A source constraint describes
its fed value instead. UI inputs keep using the adopted Lean input type while
a changed source declaration is stale. A new source type is adopted through
notebook regeneration; draft string choices do not change an active numeric
input into a string feed.

Regenerate one cell through its lifecycle controls, or all cells through the
notebook header. Inputs feed the real app API, which records the values and
returns dependent outcomes. Runtime errors stay visible with recovery controls.
Graph update timestamps include microseconds so old local feed results cannot
mask a newly adopted graph built in the same minute.

The shared textarea seeds escaped raw text during server rendering and uses a
controlled value for later edits. Saved descriptions and domain lists survive
full-page hydration without exposing placeholder comments or interpreting
HTML-like content.

## Three permission editors

Organization **Settings → Notebooks** contains effect, provider, writer-tool and
HTTP-domain ceilings. The default derives exact HTTP domains from configured
cell URLs. An explicit allowlist can instead be used; an empty explicit list
denies HTTP. Provider ceilings are optional under Advanced. They use the same
operation/resource editor as individual connections and cells.

**Settings → Connections → Connection permissions** defines an account's
ceiling. Existing unset connections use read-only defaults. Bucket, container
and CalDAV collection credentials already carry their configured base resource.
The base URL is displayed without exposing credentials. OAuth scopes remain an
additional ceiling; selecting write operations does not expand an OAuth token.

**Edit cell → Connections for this cell** selects accounts explicitly. Inherit
uses the connection ceiling within organization policy. Read-only and read/write
presets intersect these ceilings, and the app refuses explicit grants that
widen either. A provider disabled by organization policy cannot be newly selected;
an existing selection can still be removed.

For simple tasks, use a preset and the optional **Resource boundary** field.
It applies one component-wise resource root to all selected operations: for
example, an S3 prefix `reports/2026`, a Drive folder ID, a calendar ID, mailbox
ID, channel ID, repository `owner/name`, or model ID. Empty means the credential's
base resource. Storage cells also offer **Use configured object** for S3/Azure:
this infers the exact object components from the cell's path and intersects
them with the existing ceiling.

Advanced grants support independent operations, multiple resources per operation,
exact versus descendant scopes, and maximum request/response bytes. No raw JSON
is required. Read/write presets exclude delete, share, invite and send: those
are independent advanced requests. An empty operation list denies access.
All editor buttons use `type="button"`; only Save submits a policy.

Operation names come from the shared permission catalog. The UI configures
requested authority; it does not establish that a native provider adapter
implements an operation. Unsupported operations/scopes must fail closed where
effects execute and credentials are used. Runtime and broker enforcement,
capability/warrant intersections, and Lean proofs must be verified separately
before claiming an operation is supported end to end.

## Repeatable browser verification

```sh
uv run --with playwright scripts/test_notebook_ui.py --help
uv run --with playwright scripts/test_notebook_ui.py \
  --pg-bin /opt/homebrew/opt/postgresql@16/bin
```

The script starts a disposable local PostgreSQL cluster, applies every migration,
seeds fixture accounts without real credentials, builds and serves the actual
Dioxus fullstack app, and uses headless Chromium to exercise it. It accepts
port overrides and a PostgreSQL bin directory for other environments. It
blocks browser requests to external hosts. Owned processes and fixture database
files are removed afterward; screenshots, page-error results, app logs and
mock-service requests are retained in the printed artifact directory.

Coverage includes cell creation and deletion; explicit and multi-argument
dependencies; rename propagation; cycle/self/unknown-reference refusal;
declaration/name/dependency sorting; unsent value preservation; graph keyboard
and pointer navigation; generated code/signatures; zero-argument functions;
saved description/domain hydration and literal HTML-like textarea text;
regenerate-one/all lifecycle; numeric feeds and reactive output; visible
computation failure and recovery; dependency changes with rebuilt outputs;
lost-session re-registration; source type/widget changes and string choice
feeds; and a 390px layout without page overflow.

Permission coverage includes read-only connection ceilings, narrowing and byte
limit refusal, independent advanced resource grants, quick scoped presets for
storage/files/calendars/mail/workspaces/messaging/repositories/AI, connection
policy persistence to PostgreSQL and the mock vault, exact configured-object
inference, organization calendar ceilings, invalid-domain refusal, and cell
API enforcement of organization HTTP/connector ceilings.

These 24 groups are real browser/app/API/database tests, **with local writer,
runtime, credential broker and vault mocks**. Generated Lean code and native
upstream calls are not executed by this browser script.

The independent [native integration](native-connectors.md#verification) now
passes the real app → compiled Lode → broker → local Git → compiled Lun
pipeline, 655 real broker HTTP cases and 69 compiled-driver runtime cases.
Caller-owned input constraints reach the checked source contracts; invalid new
feeds leave state unchanged, and `recoverInputs:true` turns incompatible
historic values into editable errors after adoption. These checks supplement
the kernel-checked type, wiring and four-ceiling authority proofs. Provider
peers are local fixtures; live paid-provider conformance is a separate axis.
