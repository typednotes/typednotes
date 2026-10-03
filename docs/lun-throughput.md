# Compiled Lun arithmetic graph throughput and cache

Measured on 2026-10-03, macOS arm64, 16 logical CPUs. Two Nat inputs feed one
compiled `Nat + Nat → Eff [] Nat` addition through real local HTTP calls.
Both baseline and cached runs validate 1,800 results with zero failures.

- Baseline graph QPS (concurrency 1/4/8): **6.51 / 8.72 / 8.68**.
- Cached graph QPS: **562.63 / 576.73 / 2339.51**.
- Single-client graph median: **157.85 ms → 1.79 ms**.
- Cached session-update QPS: **471.84 / 1741.38 / 2066.82**.

The new bounded cache holds four checked compiled workers by default, keyed by
build/entry point/org/user/graph/schema. Graph and session operations reuse the
same immutable template; context, effect permissions, warrants, trace buffers,
inputs and session state are fresh on every request. Private request/lease and
response-ID witnesses, finite slots, deadlines, retirement and no replay protect
the runtime path. Actual permission-denial and cross-actor fixtures use this path.

Compilation (baseline 27.40 s, cached 24.98 s) and 20 sequential warm-up graph
requests are excluded. Series run in order 1/4/8: additional worker cold starts
remain in the four-client graph timing, while the eight-client run uses the warm
pool. These figures are not a steady-state scaling curve or a production capacity
guarantee. Session QPS includes setup/teardown; latency samples cover evaluated
updates. HTTPConnection objects reconnect if the server closes a connection.

Reproduce from Lun with
`python3 test/benchmark.py --temp-root /path/to/approved/scratch --requests 300 --worker-count 4`.
The script also asserts workers exit after their parent runner is terminated.

[Full methodology](https://github.com/typednotes/lun/blob/main/docs/throughput.md),
[baseline JSON](https://github.com/typednotes/lun/blob/main/test/benchmark-results-20261003.json)
and [cached JSON with source hashes](https://github.com/typednotes/lun/blob/main/test/benchmark-cached-results-20261003.json).
The cached result is from the local working tree, not a published runtime tag.
Rollout requires Linen v1.11.0 publication (Lun's SDK pin/lock are prepared) and
Lun v0.3.2 image publication; see [publication order](push-order.md).
