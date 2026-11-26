# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Releases before 0.12.2 are described in their git tags (`git tag -n`) and in
[`docs/release-*.md`](docs/).

## [Unreleased]

### Added

- `scripts/repo-sync.sh` checks live `origin` refs across sibling checkouts and
  supports fast-forward-only pull, pruning merged local branches, and guarded undo.
- Set Git author identity and signing preferences across sibling checkouts with
  `scripts/repo-sync.sh config`.
- Publish local-only topic branches with `scripts/repo-sync.sh push`; existing
  remote branches and tags are left unchanged.
- Document the proposed Lean cell package and native artifact reuse across Lun DAGs.
- Add an illustrated system overview with a global map and individual subsystem figures.

### Changed

- Run hosted browser and branding checks on version tags or explicit manual runs;
  use `bash ci/check-extended.sh` for local checks.

### Fixed

- Restore warning-free web builds on Rust 1.100 without changing URL routing.

## [0.12.2] — 2026-11-17

Released on the user's request: agent instructions and strict type checking only; no user-visible
change. See [`docs/release-0.12.2.md`](docs/release-0.12.2.md).

### Changed

- CI rejects unformatted code and any clippy warning on the server and browser builds.
- Agent instructions (`AGENTS.md`) follow the agents-md standard v3.
