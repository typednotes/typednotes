# Graph and Lode asks: implementation and verification

Verified on 2026-10-01. This closes the eight Graph/Lode questions in `TODO.md`.
The new writer bridge and LSP changes require Typednotes **v0.7.0** and Lode
**v0.4.0** to be deployed together; the earlier release images do not acquire
them retroactively.

## Can a node depend on derived nodes sharing one upstream?

Yes. A graph can fan out from one upstream, derive multiple nodes, and join them:

```text
input → root → left  ─┐
             → right ─┴→ joined → rendered
```

Each application is a typed `combineLatest` over a declared function. Arity and
argument types are checked by Lean. Caller-owned named wiring additionally
consumes the kernel-checked `WiringContract` and `BoundWiring` correspondence.
The graph builder and runtime reject cyclic/forged edges and arbitrary executable
operators; the runtime rebinds implementations to the checked declarations.

The compiled `diamond` fixture in `lun/test/e2e.sh` now checks exactly this shape:
`root = 2*x`, `left = root+1`, `right = 2*root`, `joined = left+right`.
Input `5` produces `31`; updates to `6` and `7` produce `37` and `43`.

## Can the same node emit several times?

Yes, across its live session. Feeding source occurrences recomputes the affected
dependency closure; a node can emit successive values, errors, blocked outcomes,
and recovered values. Sources can be driven by widgets, endpoints, schedules,
watches, or channel events. Refeeding an identical value produces no changed
outcome. The diamond fixture verifies three successive outputs and duplicate-value
suppression, and the existing fixtures verify error propagation and recovery.

The HTTP update returns the final outcome of each changed node for that update.
Multiple inputs in one update may cause intermediate recomputations. A function
still returns one `Eff effs T` result per invocation: autonomous infinite streams,
arbitrary in-graph `scan`/`mapM`, and streaming every intermediate result to the
browser are not claimed by this contract.

## Does Lode have enough graph documentation?

Yes. `Lode/Prompt.lean` carries the project/module layout, Lean and Linen versions,
typed function/effect examples, graph construction, `lun.json`, allowed graph
operators, and the check → publish → build → call workflow. Repository `AGENTS.md`
or `CLAUDE.md` is added as context. The notebook request supplies declarations,
named dependencies, source/output constraints, and the current capability bounds.

Lode can read the checked-out library sources and use LSP to inspect APIs instead
of inventing them. Real fixtures verify the tool-driven workflow and runtime
contracts; reliability of an arbitrary paid model's reasoning is not measured.

## Does Lode have the same restricted Eff interfaces as its implementation?

Yes, through the same compiled Lun functions/graphs. To explore a DB, graph secret,
HTTP endpoint, temporary file, or connector, Lode writes a bounded declared Lean
function, builds the published project, and invokes `lun_call` with input data.
The authenticated app supplies execution authority separately at launch/refresh.
Generated code cannot choose credentials, the actor/graph binding, or a wider
permission envelope.

`Lode/RuntimeContext.lean` has private authenticated-caller, context, bounded
transition, fresh-operation and outbound-call witnesses. Its kernel theorems
establish immutable bindings, launch/current attenuation, exact outbound-envelope
correspondence and the four independent resource ceilings. Execution consumes
these witnesses; Lun and Liaison independently enforce their existing bounds.
Policy/declaration edits revoke writer trials, including anonymous HTTP/files.
Restart preserves public ceilings but drops tokens. Refresh cannot restore removed
authority. Credential owner and executing actor remain distinct for shared accounts.

The real bridge suite passes seven groups: authorized SCRAM DB, graph-vault,
temporary files, pinned HTTP, Trace, graph and HMAC-brokered connector calls;
independent ceiling denials; identity/model injection refusal; expiry; restart;
cell revocation; and live policy revocation. See
[the exact bridge contract](https://github.com/typednotes/lode/blob/main/docs/runtime-bridge.md).

## How does Lode call tools?

Model function calls are parsed into finite `Tools.Args`, then authorized against
organization/session and selected-agent tool lists. `Tools.run` consumes
`AuthorizedArgs`; the unchecked dispatcher is private. Tool failures become
`isError:true` results, including Lun effect refusals carried in HTTP-200 JSON.

`lun_call` accepts function/graph names and input only. A private `Runtime.Call`
witness attaches the authenticated caller's declared service list, immutable
binding, narrowed ceilings and fresh warrants before `Lode.Lun.call` sends the
request. The model cannot construct a raw execution body. Extra trial services
outside the caller's declaration are refused; helpers can be private Lean code.
There is no credentialed raw SQL/HTTP fallback for a denied call.

## How does Lode test generated code?

- `check` runs the real `lake build` and returns attributable errors/warnings.
- `lsp` gives diagnostics, types, definitions, completions and goals on disk code.
- `lun_build` verifies the published commit, effect rows, JSON dictionaries,
  caller-owned output/source types and named graph wiring in the actual runner.
- `lun_call` tests representative inputs under the same bounded runtime as the
  final implementation. Denials/errors are reported rather than granted around.

These paths are tested with compiled Lode/Lun, real local Git and PostgreSQL, and
the real broker/HMAC/audit path. Vault/provider/model replies are disposable peers;
they do not establish live paid-provider conformance. Build containers, approved
libraries, filesystem/socket/TLS/SQL FFI and protected app/vault projections remain
the explicit trusted boundary.

## Does Lode have Lean LSP access?

Yes. The new `lsp` operation supports diagnostics, hover, definition, completion,
and goals through the installed Lean 4.34 `lake serve` process. Both build and
plan agents advertise it within the current allowlist. Lines and character
positions are zero-based; characters use UTF-16 units, including non-BMP checks.

Private document/request/location witnesses carry path confinement, snapshot and
cursor bounds, and a finite read-only method set. Requests/results are byte- and
time-bounded, service credentials are stripped, and complete worker process groups
are reaped. Arbitrary RPCs, commands, code actions, model-selected URIs and returned
external file content are refused. Workers are ephemeral, not a persistent daemon.
The real-server/adversarial dispatcher suite passes **63 calls**. See
[the LSP contract](https://github.com/typednotes/lode/blob/main/docs/lsp.md).

## Can Lode search, read and edit repository code?

Yes: `ls`, `grep`, `read`, `write`, and exact-match `edit` operate on the checkout.
Paths are checked against its filesystem capability; writes cannot target `.git`
or `.lake`. `grep` searches tracked and untracked source using Git. LSP provides
semantic navigation. Plan mode excludes modifying/publishing tools. Native
checkout/publication enforces repository/subtree grants and atomic expected-head
updates; deletion is independently granted.

Allowed `bash` and Lean elaboration/Lakefiles retain the container trust boundary:
named tool proofs do not make arbitrary shell execution semantically read-only.

## Reproduction

```sh
# Lun checkout; optional LUN_E2E_TMPDIR selects a scratch parent.
test/e2e.sh ../linen

# Lode checkout, with the coordinated SDK built.
lake test
test/lun.sh ../lun ../linen
python3 test/lsp.py

# App checkout; disposable PostgreSQL tools must be on PATH.
cargo test -p api --features server --lib
python3 scripts/test_native_connectors.py --temp-root "$SCRATCH" \
  --runtime --real-writer --lean-workspace "$LOCAL_LAKE_WORKSPACE"
```

Full Lun E2E: **76 checks passed**, including all four diamond/emission checks.
The local run took about 30 minutes because it compiles many separate drivers;
the CI job has a 45-minute budget. The app has **101 passing API tests**, both
server and wasm checks pass, and the whole native pipeline plus bridge passes.
