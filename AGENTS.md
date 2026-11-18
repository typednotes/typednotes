# AGENTS.md — Typednotes

<!-- agents-md standard v4 — github.com/ngrislain/agents. Refresh with the agents-md skill. -->

Instructions for coding agents (Claude Code, Codex, pi, opencode, …) and for humans working on
this project. `CLAUDE.md` imports this file; edit this one.

## Project

Typednotes is a server-side agent that acts on resources **on behalf of** users with bounded,
auditable authority. This repository is the **app**: a Dioxus 0.7 fullstack web app (`web` plus
the minimal `core` — users, identities, orgs, memberships, projects and connections in Postgres)
whose credentials and provider calls go through the sibling services `secrets` (vault) and
`liaison` (broker), with `ledger`, `lode` and `lun` for usage, code generation and execution (see
`README.md` and `docs/system-architecture.md`). Users sign in, create orgs and projects, connect
code hosts, storage, AI accounts, calendars, mail, Notion and messaging, and run notes as typed,
reactive graphs. A change is done when the checks below pass, the affected `docs/` pages, service
contracts and diagrams are updated in the same change, and connector/`Eff` permission contracts
stay in sync (see Project-specific notes).

| Task | Command |
| --- | --- |
| Install | `rustup target add wasm32-unknown-unknown`; install `dx` matching `dioxus` in `Cargo.lock` (0.7.10, see `Dockerfile`); local DB: `export DATABASE_URL=…` then `scripts/dev-db.sh`; private env: `cp .env.local.example .env.local` (see `docs/local-development.md`) |
| Type-check | `cargo check -p web --features server` and `cargo check -p web --features web --target wasm32-unknown-unknown` |
| Test | `cargo test -p api --features server --lib`; `bash ci/test-require-main-ci.sh`; `python3 ci/test-workflow-policy.py` (needs PyYAML); `python3 scripts/test_logo.py` (needs fontTools, Fira Code, rsvg); opt-in end-to-end: `uv run --with playwright scripts/test_notebook_ui.py --help`, `scripts/test_native_connectors.py` |
| Lint / format | `cargo fmt --all --check`; `cargo clippy -p web --features server --all-targets -- -D warnings`; `cargo clippy -p api --features server --all-targets -- -D warnings`; `cargo clippy -p web --features web --target wasm32-unknown-unknown --all-targets -- -D warnings` |
| Build / release | `dx bundle -p web --web --release` (see `Dockerfile`); release: a `vX.Y.Z` tag publishes the image via `.github/workflows/docker-publish.yml`, gated by `ci/require-main-ci.sh` (see Permissions before pushing) |
| Run (local app) | `bash scripts/dev.sh --check`, then `bash scripts/dev.sh` (`dx serve -p web`, http://localhost:8080); `TODO(agents-md): no CLI entry point` |

Before declaring a change done, run type-check, tests and lint, and report their real output.

## Permissions: what to do freely, when asked, and only after confirming

**Do freely** — reversible and local:
- read, search, and run type-checkers, tests, linters, formatters and local builds;
- edit files in the working tree within the scope of the task, including `docs/`, `reviews/`,
  `NOTES.md`, `TODO.md`, specs and tests;
- create scratch files in git-ignored directories (`output/`, `.cache/`, `tmp/`);
- read shell history and logs to spot repetitive work (see "Suggest automating repeated work").

**Only when the user asks** — do not initiate:
- `git commit`, creating or switching branches, stashing, rebasing a local branch;
- adding, removing or upgrading dependencies (propose the change and why);
- changing the machine outside the repository — global installs, dotfiles, symlinks, services,
  shell profiles — and recording each change in the manifest (see "Manifest of system changes");
- runs that are long (> ~10 min), costly (GPU, cluster, cloud, paid APIs) or touch shared
  resources (shared caches, staging data);
- clearing `.cache/` or `output/`, or forcing a cache refresh, when the content is slow or costly
  to produce again.

**Confirm explicitly every time**, even if a similar action was approved earlier — approval does
not carry over to a new context:
- `git push` of any kind; never force-push, and never push to `main`/`master` directly;
- opening, merging or closing a PR/MR, posting review comments, tagging, releasing, publishing a
  package or image;
- deploying to any environment (dev, staging, prod);
- publishing outside the repo: Confluence, Google Docs/Sheets, Slack, email, issue trackers;
- deleting or overwriting anything not recreated by a command in this repo: branches, remote
  resources, datasets, buckets, tables, experiment outputs;
- changing infrastructure, permissions, secrets or credentials.

**Never**: commit secrets or customer data; skip hooks (`--no-verify`); rewrite pushed history;
weaken a type-check, a test or a lint rule to make it pass; claim a check passed without running it.

## Repository layout and the documents it keeps

| Path | Audience | Content | Keep in sync with |
| --- | --- | --- | --- |
| `docs/` | humans, high level | project, architecture, algorithms, code map — a hierarchy of Markdown files illustrated with SVG figures | the code, every change |
| `README.md` (root and per package) | users and contributors, close to the code | install, usage, examples, how the code is organised | the code it sits next to |
| `SPEC.md` (per module/package) | humans first | the contract the code must fulfil | types and tests check it |
| `reviews/` (git-ignored) | reviewers | reviews of commits and PRs: explanation, SVG figures, suggested comments and fixes | — (dated snapshots) |
| `output/` (git-ignored) | users of the application | large, binary or reproducible outputs of runs | — (regenerated by the CLI) |
| `.cache/` (git-ignored) | the code | cached results of slow downloads and computations | — (safe to delete) |
| `CHANGELOG.md` | users of a released artifact | one entry per version, newest on top | every release |
| `LOGBOOK.md` | humans re-running experiments | one entry per experiment, newest on top, with repeatability info | every experiment |
| `TODO.md` | everyone | planned but not executed work | when planning or finishing work |
| `NOTES.md` (anywhere) | humans and agents | things worth knowing that are not definitive documentation | when something surprising is learned |

### `docs/` — high-level documentation

- A hierarchy with an index at `docs/README.md` linking every page. Typical sections:
  `docs/project/` (goals, scope, decisions, roadmap), `docs/architecture/` (components, data
  flow, deployment), `docs/algorithms/` (methods, math, complexity, trade-offs), `docs/code/`
  (module map, key types, extension points).
- Illustrate with SVG figures stored next to the page that uses them (`docs/architecture/figures/`).
  SVG is the editable source: hand-written or generated by a committed script, legible at page
  width, in light and dark backgrounds, with a `<title>` and `<desc>`. Do not commit rendered PNGs.
- `docs/` explains *why* and *how it fits together*; `README.md` files explain *how to use and
  modify* the code next to them. Link rather than repeat; one canonical page per subject.
- Keep `docs/` in sync: a change to behaviour, architecture, an algorithm or a public interface
  updates the affected pages and figures **in the same change**. Separate measured results from
  expectations and proposals.

### `SPEC.md` — one contract per module/package

- Every module, package or component with a public surface has a `SPEC.md` beside its code: a short,
  human-readable statement of what it guarantees — purpose, inputs/outputs, invariants, error
  behaviour, performance bounds, what it explicitly does not do.
- The spec is the source of truth. Types encode as much of it as the language allows; tests check
  the rest. When code and spec disagree, fix one of them deliberately and say which.
- Number the clauses (`S1`, `S2`, …) so tests and reviews can cite them.

### `reviews/` — reviews of commits and PRs

One directory per review, `reviews/YYYY-MM-DD-<slug>/`, with a `README.md` (summary, walkthrough,
findings ranked by severity, suggested comments ready to paste, suggested fixes as diffs) and
`figures/*.svg`. A video only when the user asks for one. `reviews/` is **git-ignored**: reviews
are local working documents, not project history. Posting them anywhere requires confirmation
(see Permissions).

### `output/` — generated results, git-ignored

What an application produces that is large, not text (images, videos, models, archives, binary
tables) or reproducible from the code and its inputs goes to `output/`, which is **git-ignored**.
Organise it by command and run (`output/<command>/<run-id>/`), write next to each result what
produced it (command, parameters, commit), and make the location a CLI option with `output/` as
the default. Anything worth keeping is either reproducible from the repo (then it stays in
`output/`) or published deliberately (a release, a bucket, a document — see Permissions); small,
reviewed, hand-curated text results may be committed elsewhere, e.g. in `docs/`.

### `.cache/` — caching slow objects, git-ignored

Anything slow or costly to obtain — a large download, a long computation, a remote query — is
cached under `.cache/` (**git-ignored**), with the most idiomatic library of the language: in
Python `joblib.Memory` for function results, Parquet (or JSON for small, human-readable data) for
tables keyed by their parameters; elsewhere the equivalent (`cacache`/file caches in JS, `cached`
or file caches in Rust, …).
- The cache key covers everything that determines the result: parameters, input versions or
  checksums, and a code version. Cache only results that cannot change for a given key — never
  "the latest" of something live (pin a date, a version or a completed time window instead).
- Deleting `.cache/` must always be safe: it costs time, never correctness. Downloads are
  verified (checksum or size) before being cached.
- The CLI offers `--no-cache` and `--refresh`, and the location is configurable (config file or
  environment variable), with `.cache/` as the default.

### `CHANGELOG.md` — only if the project is released as a versioned artifact

Library, image, executable, model: one section per version, newest on top, following
[Keep a Changelog](https://keepachangelog.com) and [SemVer](https://semver.org). Unreleased changes
accumulate under `## [Unreleased]`. Bump the version and the changelog in the same change.

### `LOGBOOK.md` — only if the project runs experiments

One entry per experiment, newest on top, readable by a human who was not there: date, question,
setup, result (with numbers), conclusion, and how to repeat it — commit hash, exact command,
parameters, seeds, data snapshot, links to outputs and dashboards. Failed and inconclusive
experiments are logged too.

### `TODO.md` and `NOTES.md`

- `TODO.md`: planned but not executed work, as a checklist grouped by theme, with enough context to
  pick an item up cold. Remove items when done (the history is in git and the changelog).
- `NOTES.md`: anywhere something should be known by a human or an agent and is not definitive
  documentation — gotchas, findings, dead ends, external constraints, open questions. Promote a note
  to `docs/` or a `README.md` once it becomes definitive.

## Code

### Types: use the strongest standard checking the language offers, maximally

- Use the standard type-checker of the language/framework in its strictest practical mode, run it in
  CI, and keep it at zero errors. Here: Rust (edition 2021, Dioxus 0.7.10), with
  `cargo clippy --all-targets -- -D warnings` (default lint groups plus `clippy.toml`'s
  `await-holding-invalid-types` for Dioxus signal borrows held across `.await`; warnings fail the
  build) and `cargo fmt --all --check` in CI. Clippy covers what CI builds: the server
  (`-p web` and `-p api`, `--features server`) in the check job and the browser build
  (`-p web --features web --target wasm32-unknown-unknown`) in the extended job; the `desktop`
  and `mobile` packages are not built in CI. A `#[allow(clippy::…)]` needs a comment justifying it.
- Make illegal states unrepresentable: precise types, enums/sum types over strings and flags, new
  types for identifiers and units, immutable data by default. Avoid `Any`, casts and ignores; when
  one is unavoidable, scope it narrowly and justify it in a comment.

### Modularity and docstrings

- Organise the code in cohesive modules with explicit, minimal public interfaces and acyclic
  dependencies. One canonical implementation per concern; delete dead paths rather than keep them.
- Every public module, class and function has a docstring written for a human user: what it is
  for, how to call it, what it returns or raises, and an example when usage is not obvious. Explain
  *why* in comments, not *what*.

### One CLI per application

An application project (anything run rather than imported) exposes **a single command-line entry
point** with a short name, e.g. `uv run <cli> <command>` in Python (a `[project.scripts]` entry),
`npx <cli>`, `cargo run --bin <cli>`. Every operation — run, train, evaluate, serve, inspect,
publish — is a subcommand of it, not a loose script.

- **Self-documented**: `<cli>`, `<cli> --help` and `<cli> <command> --help` at every level list
  the commands, arguments, defaults and an example. Running a command with no arguments prints help
  rather than doing something surprising.
- **No dead ends on missing arguments**: each argument either has a default or is asked for
  interactively. Defaults live preferably in a TOML config (`<cli>.toml`, or `[tool.<cli>]` in
  `pyproject.toml`), with hard-coded fallbacks in the code; precedence is
  flag > environment variable > config file > hard-coded default, and `--help` shows the effective
  value and where it came from.
- Interactive prompts only when stdin is a TTY; otherwise (CI, agents, pipes) fail fast with a message
  naming the missing flag. Destructive or outward-facing commands ask for confirmation and accept
  `--yes` and `--dry-run`.
- **Rich, colourful terminal UI** preferably: coloured and structured help, tables, progress bars,
  spinners, syntax-highlighted output and interactive prompts/selections (Python: `rich` +
  `rich-argparse`, Typer, Textual for full TUIs; JS: `@clack/prompts`, `ink`; Rust: `clap` with
  colours, `indicatif`, `ratatui`; Go: Charm's `bubbletea`/`lipgloss`). Degrade gracefully: plain
  output when not a TTY, and honour `NO_COLOR`.
- Large, binary or reproducible results go to `output/` (overridable with `--output`); slow
  intermediate results are cached in `.cache/` (`--no-cache`, `--refresh`) — see above.
- Data goes to stdout, logs and progress to stderr; exit codes are non-zero on failure. Offer
  `--json` where the output is consumed by other programs.

### Tests

- Tests do two jobs: **illustrate** every unit and higher-level functionality working (they are
  executable examples — readable, named after the behaviour), and **check** that the code fulfils its
  `SPEC.md` (cite clause ids, e.g. `test_s3_rejects_empty_window`).
- Cover unit, integration and end-to-end levels as relevant; prefer property-based tests for
  invariants. Every bug fix comes with a regression test. Tests are deterministic and fast by default;
  slow or networked tests are marked and opt-in.

## Working style

- Keep changes focused on the task; mention unrelated issues in `TODO.md` or `NOTES.md` instead of
  fixing them silently.
- Update code, types, tests, `SPEC.md`, `docs/`, `README.md`, `CHANGELOG.md` and figures together.
- Report faithfully: what was run, what passed, what failed, what was skipped.

### Manifest of system changes

Changes to the machine outside the repository are recorded so they can be tracked and reverted:
installing or upgrading tools globally (`brew`, `uv tool`, `pipx`, `npm -g`, `cargo install`),
editing dotfiles and configuration under `$HOME`, creating symlinks, services, launch agents or
cron jobs, changing shell profiles or system settings. (Changes inside the repo are tracked by
git; cloud resources by their own infrastructure-as-code.)
- **Where**: the XDG state directory, `${XDG_STATE_HOME:-~/.local/state}/<tool>/` — an
  application uses its own name; an agent acting by hand uses `agents/`, appending one JSON
  object per line to `manifest.jsonl`:
  `{"time": "…", "project": "…", "command": "…", "changes": ["…"], "revert": "…"}`.
- **What**: the exact command, every path or setting it changed, and what reverting needs — the
  inverse command, the backup path or the previous value. Back up a file before overwriting it.
- **Applications** that make such changes (installers, linkers, setup commands) write the
  manifest themselves and offer `history` and `revert` subcommands. A revert undoes newest first,
  skips an action whose path changed since, and is itself recorded.

### Undo log for mutating actions

An application whose actions mutate state — files, configuration, links, services, databases,
anything not regenerated by a command — keeps an **undo log**: one record per mutation, enough to
revert it.
- **At the scope of what changed**, so the log sits next to the state it can restore:

  | Scope of the mutation | Undo log |
  | --- | --- |
  | system-wide (`/etc`, `/usr/local`, system services) | `/var/lib/<app>/undo.jsonl` (Linux), `/Library/Application Support/<app>/undo.jsonl` (macOS) |
  | user-wide (`$HOME`, dotfiles, user services) | `${XDG_STATE_HOME:-~/.local/state}/<app>/undo.jsonl` |
  | project or folder (files under a working directory) | `<folder>/.<app>/undo.jsonl`, git-ignored — not in `.cache/`, which must stay safe to delete |

  An action spanning several scopes writes a record in each.
- **A record** holds: id, time, the command and run it belongs to, the target (path, key, row),
  its state before (a backup path, the previous value, or "absent") and after, and the inverse
  operation. Back up anything overwritten or deleted, next to the log (`backups/<id>/`).
- **Write-ahead**: write the record *before* mutating and mark it done after, so an interrupted
  action can still be undone or completed.
- **Undo** (`<cli> undo [id|run]`, default: the latest run): newest first, only if the target
  is still in its recorded "after" state — otherwise skip and say why; the undo is itself
  logged. Backups are kept until the user prunes them (`<cli> undo --prune …`).

### Suggest automating repeated work

Look out for tasks the user does by hand again and again, and propose to automate them.
- **Where to look**: the current session (the same commands or edits requested several times);
  the shell history (`~/.zsh_history`, `~/.bash_history`), for commands touching this project;
  CI and run logs; `LOGBOOK.md`, `NOTES.md` and `README.md` steps that are long manual sequences;
  `git log` (recurring chore commits, repeated fix-ups).
- **History and logs are sensitive**: read them locally and only for this purpose, extract
  command *patterns*, and never copy secrets, tokens, hostnames of customers or personal data into
  files, messages or suggestions.
- **Suggest, don't impose**: at a natural break, propose briefly what is repeated, how often, the
  automation (preferably a subcommand of the project's CLI; otherwise a script, a git hook, a CI
  job, a scheduled job or an agent skill) and what it saves. Implement it only if the user agrees;
  note accepted-but-deferred ideas in `TODO.md`, and do not repeat a suggestion the user declined.

## Project-specific notes

### Git workflow

An explicit user request to push a branch or to open or update a pull request counts as
confirmation for the requested operation when its scope is clear. Ask only if the scope is
unclear; do not ask twice for the same operation. Pushing a branch does not authorize opening
a pull request, and opening one does not authorize merging it. Follow the Permissions section
above: use a topic branch, never push directly to `main`/`master`, and never force-push.

### Documentation and code synchronization

**Keep documentation in sync with the code in the same change.** Documentation
review is part of completing implementation work, not a later cleanup task.

- Update relevant `docs/*.md`, service contracts and README entry points when
  behavior, APIs, wire/storage schemas, permissions, ownership, deployment or
  recovery semantics change. Add a document when a new contract has no home.
- Keep architecture diagrams and their explanatory text synchronized with the
  implementation. Update SVG arrows, boundaries and labels together, and check
  rendering, text fit, accessibility descriptions and links after editing.
- Use `docs/system-architecture.md` as the implemented-system overview. Trace
  claims to the owning code/service; do not copy a historical design as if it
  were the current implementation.
- Distinguish proposed, implemented, locally verified, published and deployed
  behavior. Label historical decision records and incomplete deployment wiring.
- Keep proof claims, tests, operational assumptions and remaining trusted
  boundaries explicit. References to papers or similar systems explain a design;
  they do not transfer those systems' guarantees to Typednotes.

### Connector capabilities and `Eff` permissions

**Keep connector possibilities and effect-level permissions in sync.** Adding,
removing, or changing a connector operation must update its capability contract,
permission catalog, presets, UI, runtime/broker enforcement, and documentation
together. This applies to every connector: AI, repositories, S3/Azure, Drive/Dropbox,
calendars, email, Notion, and messaging—not just anonymous HTTP.

- Model permissions as **operations plus structured resource scopes**, not a
  single read/write flag. Distinguish, for example, object read/write/delete and
  bucket prefixes; file read/update/share and folders; event read/create/delete
  and calendars; message read/draft/send/delete and mailboxes or recipients.
- Effective authority is the intersection of organization policy, connection
  permissions, the cell's `Eff` capability, and its warrant. A cell, generated
  implementation, or session update must never widen any of these bounds.
- Enforce permissions where effects execute in `lun` and where credentials are
  used in `liaison`; UI validation and instructions to the model are not enforcement.
  Unknown or unsupported operations/scopes must fail closed; never fall back to
  unrestricted HTTP, raw IO, or a broader credential.
- Make common setups easy: provide sensible, narrowly scoped presets and infer
  resource boundaries from the chosen bucket, folder, calendar, or cell URL.
  Keep advanced grants available without making them mandatory for simple tasks.
- Preserve hard organization/user boundaries for database schemas and temporary
  files. Authentication/OAuth scopes are additional ceilings, not permission to
  bypass the application's capability model.
- Add drift/coverage tests for every advertised operation, presets, scope
  boundaries, narrowing, and denied calls. Verify the actual runtime/broker path
  before marking an operation or permission TODO complete; update dependent
  service contracts and migrations when their wire/storage shapes change.

### Guarantees through Lean's type system

**Whenever a guarantee is required, encode it in Lean's types and proofs—not
only in tests, conventions, UI checks, or instructions to a model.** Make invalid
states and unauthorized operations unrepresentable where possible.

- Carry the required evidence in the operation's type: scoped resources,
  operation permissions, verified warrants, bounded authority, and validated
  state transitions. Keep constructors private when callers must not forge it.
- For runtime inputs, validate at the boundary and return a typed witness or a
  structured refusal. Execution must consume that witness; it must not bypass it
  through raw strings, unrestricted IO, or a broader alternative API.
- Prove the properties the design promises, including scope confinement,
  narrowing/attenuation, and authority intersections. Do not label a guarantee
  complete merely because a test suite passes.
- Use tests in addition to types and proofs to verify parsers, integrations,
  transports, FFI, and the correspondence between the proved model and actual
  execution. State any remaining trusted boundary explicitly.

### Dioxus 0.7 guide

You are an expert [0.7 Dioxus](https://dioxuslabs.com/learn/0.7) assistant. Dioxus 0.7 changes every api in dioxus. Only use this up to date documentation. `cx`, `Scope`, and `use_state` are gone

**CRITICAL: Always consult https://dioxuslabs.com/learn/0.7/ before writing Dioxus code.** Follow the official documentation carefully — do not guess at APIs or patterns. When unsure about an API, look it up in the docs first.

Provide concise code examples with detailed descriptions

#### Dioxus Dependency

You can add Dioxus to your `Cargo.toml` like this:

```toml
[dependencies]
dioxus = { version = "0.7.1" }

[features]
default = ["web", "webview", "server"]
web = ["dioxus/web"]
webview = ["dioxus/desktop"]
server = ["dioxus/server"]
```

#### Launching your application

You need to create a main function that sets up the Dioxus runtime and mounts your root component.

```rust
use dioxus::prelude::*;

fn main() {
	dioxus::launch(App);
}

#[component]
fn App() -> Element {
	rsx! { "Hello, Dioxus!" }
}
```

Then serve with `dx serve`:

```sh
curl -sSL http://dioxus.dev/install.sh | sh
dx serve
```

#### UI with RSX

```rust
rsx! {
	div {
		class: "container", // Attribute
		color: "red", // Inline styles
		width: if condition { "100%" }, // Conditional attributes
		"Hello, Dioxus!"
	}
	// Prefer loops over iterators
	for i in 0..5 {
		div { "{i}" } // use elements or components directly in loops
	}
	if condition {
		div { "Condition is true!" } // use elements or components directly in conditionals
	}

	{children} // Expressions are wrapped in brace
	{(0..5).map(|i| rsx! { span { "Item {i}" } })} // Iterators must be wrapped in braces
}
```

#### Assets

The `asset!()` macro links to local files. Paths start with `/` and are relative to the package root. **Files are content-hashed at build time** — this means relative references between assets (e.g., CSS `url(../fonts/...)`) will break.

```rust
rsx! {
	img {
		src: asset!("/assets/image.png"),
		alt: "An image",
	}
}
```

##### Public Directory

Files placed in `public/` at the package root are served **as-is without hashing**. Use this for third-party assets that have internal relative references (e.g., Font Awesome CSS + webfonts):

```
packages/web/public/
└── fontawesome/
    ├── css/all.min.css      ← served at /fontawesome/css/all.min.css
    └── webfonts/             ← CSS references ../webfonts/ — works because paths are preserved
```

Reference public files with a plain path (not `asset!()`):
```rust
document::Link { rel: "stylesheet", href: "/fontawesome/css/all.min.css" }
```

##### Styles

The `document::Stylesheet` component will inject the stylesheet into the `<head>` of the document

```rust
rsx! {
	document::Stylesheet {
		href: asset!("/assets/styles.css"),
	}
}
```

#### Components

Components are the building blocks of apps

* Component are functions annotated with the `#[component]` macro.
* The function name must start with a capital letter or contain an underscore.
* A component re-renders only under two conditions:
	1.  Its props change (as determined by `PartialEq`).
	2.  An internal reactive state it depends on is updated.

```rust
#[component]
fn Input(mut value: Signal<String>) -> Element {
	rsx! {
		input {
            value,
			oninput: move |e| {
				*value.write() = e.value();
			},
			onkeydown: move |e| {
				if e.key() == Key::Enter {
					value.write().clear();
				}
			},
		}
	}
}
```

Each component accepts function arguments (props)

* Props must be owned values, not references. Use `String` and `Vec<T>` instead of `&str` or `&[T]`.
* Props must implement `PartialEq` and `Clone`.
* To make props reactive and copy, you can wrap the type in `ReadOnlySignal`. Any reactive state like memos and resources that read `ReadOnlySignal` props will automatically re-run when the prop changes.

#### State

A signal is a wrapper around a value that automatically tracks where it's read and written. Changing a signal's value causes code that relies on the signal to rerun.

##### Local State

The `use_signal` hook creates state that is local to a single component. You can call the signal like a function (e.g. `my_signal()`) to clone the value, or use `.read()` to get a reference. `.write()` gets a mutable reference to the value.

Use `use_memo` to create a memoized value that recalculates when its dependencies change. Memos are useful for expensive calculations that you don't want to repeat unnecessarily.

```rust
#[component]
fn Counter() -> Element {
	let mut count = use_signal(|| 0);
	let mut doubled = use_memo(move || count() * 2); // doubled will re-run when count changes because it reads the signal

	rsx! {
		h1 { "Count: {count}" } // Counter will re-render when count changes because it reads the signal
		h2 { "Doubled: {doubled}" }
		button {
			onclick: move |_| *count.write() += 1, // Writing to the signal rerenders Counter
			"Increment"
		}
		button {
			onclick: move |_| count.with_mut(|count| *count += 1), // use with_mut to mutate the signal
			"Increment with with_mut"
		}
	}
}
```

##### Context API

The Context API allows you to share state down the component tree. A parent provides the state using `use_context_provider`, and any child can access it with `use_context`

```rust
#[component]
fn App() -> Element {
	let mut theme = use_signal(|| "light".to_string());
	use_context_provider(|| theme); // Provide a type to children
	rsx! { Child {} }
}

#[component]
fn Child() -> Element {
	let theme = use_context::<Signal<String>>(); // Consume the same type
	rsx! {
		div {
			"Current theme: {theme}"
		}
	}
}
```

#### Async

For state that depends on an asynchronous operation (like a network request), Dioxus provides a hook called `use_resource`. This hook manages the lifecycle of the async task and provides the result to your component.

* The `use_resource` hook takes an `async` closure. It re-runs this closure whenever any signals it depends on (reads) are updated
* The `Resource` object returned can be in several states when read:
1. `None` if the resource is still loading
2. `Some(value)` if the resource has successfully loaded

```rust
let mut dog = use_resource(move || async move {
	// api request
});

match dog() {
	Some(dog_info) => rsx! { Dog { dog_info } },
	None => rsx! { "Loading..." },
}
```

#### Routing

All possible routes are defined in a single Rust `enum` that derives `Routable`. Each variant represents a route and is annotated with `#[route("/path")]`. Dynamic Segments can capture parts of the URL path as parameters by using `:name` in the route string. These become fields in the enum variant.

The `Router<Route> {}` component is the entry point that manages rendering the correct component for the current URL.

You can use the `#[layout(NavBar)]` to create a layout shared between pages and place an `Outlet<Route> {}` inside your layout component. The child routes will be rendered in the outlet.

```rust
#[derive(Routable, Clone, PartialEq)]
enum Route {
	#[layout(NavBar)] // This will use NavBar as the layout for all routes
		#[route("/")]
		Home {},
		#[route("/blog/:id")] // Dynamic segment
		BlogPost { id: i32 },
}

#[component]
fn NavBar() -> Element {
	rsx! {
		a { href: "/", "Home" }
		Outlet<Route> {} // Renders Home or BlogPost
	}
}

#[component]
fn App() -> Element {
	rsx! { Router::<Route> {} }
}
```

```toml
dioxus = { version = "0.7.1", features = ["router"] }
```

#### Fullstack

Fullstack enables server rendering and ipc calls. It uses Cargo features (`server` and a client feature like `web`) to split the code into a server and client binaries.

```toml
dioxus = { version = "0.7.1", features = ["fullstack"] }
```

##### Server Functions

Use the `#[post]` / `#[get]` macros to define an `async` function that will only run on the server. On the server, this macro generates an API endpoint. On the client, it generates a function that makes an HTTP request to that endpoint.

```rust
#[post("/api/double/:path/&query")]
async fn double_server(number: i32, path: String, query: i32) -> Result<i32, ServerFnError> {
	tokio::time::sleep(std::time::Duration::from_secs(1)).await;
	Ok(number * 2)
}
```

##### Hydration

Hydration is the process of making a server-rendered HTML page interactive on the client. The server sends the initial HTML, and then the client-side runs, attaches event listeners, and takes control of future rendering.

###### Errors
The initial UI rendered by the component on the client must be identical to the UI rendered on the server.

* Use the `use_server_future` hook instead of `use_resource`. It runs the future on the server, serializes the result, and sends it to the client, ensuring the client has the data immediately for its first render.
* Any code that relies on browser-specific APIs (like accessing `localStorage`) must be run *after* hydration. Place this code inside a `use_effect` hook.

#### Dioxus Components Library

**Always prefer the dx components library over hand-rolled HTML elements.** Components are in `packages/ui/src/components/` (source from https://github.com/DioxusLabs/components).

##### Available Components

Import from `ui::components`:

| Component | Variants | Use for |
|-----------|----------|---------|
| `Button` | `Primary`, `Secondary`, `Destructive`, `Outline`, `Ghost` | All buttons |
| `Input` | — | Text inputs, email, password, number fields |
| `Textarea` | `Default`, `Fade`, `Outline`, `Ghost` | Multi-line text input |
| `Label` | — | Accessible form labels (requires `html_for` prop) |
| `Select` | — | Dropdowns (with `SelectTrigger`, `SelectValue`, `SelectList`, `SelectOption`) |
| `Avatar` | Size: `Small`, `Medium`, `Large` | User avatars (with `AvatarImage`, `AvatarFallback`) |
| `Badge` | `Primary`, `Secondary`, `Destructive`, `Outline` | Tags, status indicators |
| `ToastProvider` | — | Wraps app for toast support |
| `use_toast()` | — | Hook to dispatch toasts |

##### Important Notes

- When using `Input` or `Textarea` with `oninput`, always annotate the closure parameter type: `move |evt: FormEvent| ...` — Rust cannot infer the type from `Option<EventHandler<FormEvent>>`.
- The `Label` component requires `html_for: "element-id"` to associate with an input.
- `ToastProvider` must wrap the app (already done in `web/src/main.rs`).
- Toast usage: `let toast_api = use_toast(); toast_api.success("Saved".to_string(), ToastOptions::new());`
