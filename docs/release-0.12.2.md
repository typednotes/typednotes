# Typednotes v0.12.2 — strict lint and format checks

Maintenance patch on v0.12.1, released on the user's request: agent instructions and strict type
checking only; no user-visible change. Runtime behavior, service APIs, dependency pins and
migration requirements are unchanged.

- CI runs `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` over the
  targets it already builds: the server (`-p web` and `-p api`, `--features server`) in the check
  job and the browser build (`-p web --features web --target wasm32-unknown-unknown`) in the
  extended job. `clippy.toml`'s `await-holding-invalid-types` is now enforced. The `desktop` and
  `mobile` packages are still not built in CI.
- The workspace is formatted with `rustfmt`, and the six clippy findings are fixed without
  behavior changes; the two `too_many_arguments` allowances on authority-taking server functions
  carry a justification.
- `AGENTS.md` follows the agents-md standard v3, keeping the stricter "never push, no pull
  requests" rule. `CHANGELOG.md` is added.

Local checks passed: `cargo fmt --all --check`, the three clippy commands above,
`cargo test -p api --features server --lib`, `bash ci/test-require-main-ci.sh` and
`python3 ci/test-workflow-policy.py`. This patch adds no live-provider or deployment
verification evidence.

Deployment requirements are those of [v0.12.0](release-0.12.0.md): matching **Liaison v0.7.0**,
app migration **0016** and configured persistent encrypted device storage for paired messaging.

Publish the intended latest main commit and its v0.12.2 tag together, then verify exact-commit
main CI before image promotion. Do not move existing tags to another commit. Source/tag
publication is separate from cloud deployment.
