# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Releases before 0.12.2 are described in their git tags (`git tag -n`) and in
[`docs/release-*.md`](docs/).

## [Unreleased]

## [0.12.2] — 2026-11-17

Released on the user's request: agent instructions and strict type checking only; no user-visible
change. See [`docs/release-0.12.2.md`](docs/release-0.12.2.md).

### Changed

- CI rejects unformatted code and any clippy warning on the server and browser builds.
- Agent instructions (`AGENTS.md`) follow the agents-md standard v3.
