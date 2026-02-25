# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

TypedNotes is a cross-platform note-taking application built with Dioxus 0.7.1 that compiles to Web, Desktop, and Mobile from a single Rust codebase. It uses a fullstack architecture with Axum on the server, PostgreSQL for storage, and OAuth (GitHub/Google) for authentication.

## Build Commands

```bash
# Serve for development (run from package directory)
cd packages/web && dx serve        # Web (fullstack with server)
cd packages/desktop && dx serve    # Desktop (webview)
cd packages/mobile && dx serve --platform android  # Android
cd packages/mobile && dx serve --platform ios      # iOS

# Production bundle (used in Docker build)
dx bundle --fullstack --release --debug-symbols=false

# Update dependencies
make update

# Generate .env from encrypted secrets
make env

# Database migrations (requires DATABASE_URL in .env)
make migrate-run       # Run pending migrations
make migrate-revert    # Revert last migration

# Infrastructure (Scaleway via OpenTofu)
make infra-up      # Deploy and save outputs to secrets.yaml
make infra-down    # Destroy infrastructure
```

## Architecture

```
packages/
├── ui/       # Shared UI components and auth context (lib crate)
├── api/      # Server functions and data models (lib crate)
├── web/      # Web platform entry point (fullstack Axum server)
├── desktop/  # Desktop platform (webview, no server)
└── mobile/   # Mobile platform (iOS/Android, no server)
infra/        # OpenTofu/Terraform for Scaleway
container/    # Dockerfile for production build
```

**Key patterns:**
- Shared components live in `ui`, server logic in `api`, platform-specific routing in `web`/`desktop`/`mobile`
- Each platform defines its own `Route` enum and wraps the generic `Navbar` with platform-specific `Link` components
- Only `web` runs a full Axum server with OAuth callbacks and session middleware; desktop/mobile are simplified clients

## Feature Gate Strategy

Code is split into server and client via Cargo features:

- **`server` feature** — Gates all Axum, SQLx, OAuth, and session code. Enabled only for the server binary.
- **`web` / `desktop` / `mobile` features** — Select the client platform renderer.
- Server-only dependencies in `Cargo.toml` use `optional = true` and are activated by the `server` feature.
- Use `#[cfg(feature = "server")]` to guard server-only modules and imports.
- Use `#[cfg(target_arch = "wasm32")]` for browser-specific code (e.g., `web_sys::window()`).

## Authentication & Sessions

- OAuth flow: `LoginButton` → `get_login_url()` → redirect to provider → callback at `/auth/{provider}/callback` → session stored in PostgreSQL via `tower-sessions`
- `AuthProvider` component provides `Signal<AuthState>` via context; access with `use_auth()` hook
- Server functions that need the session use `#[get("/path", session: tower_sessions::Session)]`
- User model: `User` (server-side, full DB record) and `UserInfo` (client-safe, serializable)

## Database

- PostgreSQL via SQLx with compile-time query verification
- Connection pool: lazy singleton via `OnceCell` in `api/src/db/pool.rs`, configured from `DATABASE_URL`
- Migrations live in `packages/api/migrations/` and are **automatically run on server startup** (the server is the sole migration runner in production). `make migrate-run` is a dev convenience only — avoid using it against the production DB
- Install sqlx-cli: `cargo install sqlx-cli --no-default-features --features postgres,rustls`

## Secrets Management

- Secrets encrypted with SOPS + GPG in `secrets.yaml`
- `make env` decrypts and generates `.env` for local development
- `make tfvars` generates `infra/terraform.tfvars` for infrastructure deployment
- Required tools: `sops`, `yq`, GPG key configured

## Dioxus 0.7 API (Important)

**CRITICAL: Always consult https://dioxuslabs.com/learn/0.7/ before writing Dioxus code.** Follow the official documentation carefully — do not guess at APIs or patterns. See `AGENTS.md` for a local quick reference. Key points:

- **No `cx`, `Scope`, or `use_state`** — these are removed in 0.7
- Use `use_signal(|| value)` for local state, call like function to read: `count()`
- Use `use_memo(move || ...)` for derived values
- Use `use_resource` for async operations, `use_server_future` for fullstack (ensures server renders data before hydration)
- Server functions use `#[post("/path")]` / `#[get("/path")]` macros
- Assets: `asset!("/assets/file.png")` — paths relative to package root; hashes filenames at build time
- **`public/` directory**: Files in `packages/<pkg>/public/` are served as-is without hashing — use this for third-party assets with internal relative references
- **Icons**: Use `dioxus-free-icons` (Font Awesome solid set) via `ui::Icon` and `ui::icons::*` — inline SVGs, no external CSS needed. Example: `Icon { icon: FaGear, width: 14, height: 14 }`
- Router: `#[derive(Routable)]` enum with `#[route]` and `#[layout]` attributes
- Context: `use_context_provider(|| value)` to provide, `use_context::<Type>()` to consume

## UI Components (Dioxus Components Library)

This project uses the official Dioxus components library (`dioxus-primitives` from https://github.com/DioxusLabs/components). **Always prefer these components over hand-rolled HTML elements.**

- Components live in `packages/ui/src/components/` with their styles
- Theme CSS: `packages/ui/assets/dx-components-theme.css` (loaded globally via `ui::DX_COMPONENTS_CSS`)
- `ToastProvider` wraps the app in `packages/web/src/main.rs`

**Available components** (import from `ui::components`):
- `Button` with `ButtonVariant::{Primary, Secondary, Destructive, Outline, Ghost}`
- `Input` — styled text input (use `oninput: move |evt: FormEvent| ...` for type inference)
- `Textarea` with `TextareaVariant::{Default, Fade, Outline, Ghost}`
- `Label` — accessible form label (`html_for` prop required)
- `Select`, `SelectTrigger`, `SelectValue`, `SelectList`, `SelectOption` — accessible select
- `Avatar`, `AvatarImage`, `AvatarFallback` with `AvatarImageSize::{Small, Medium, Large}`
- `Badge` with `BadgeVariant::{Primary, Secondary, Destructive, Outline}`
- `ToastProvider`, `use_toast()`, `ToastOptions` — toast notifications

**Usage pattern:**
```rust
use ui::components::{Button, ButtonVariant, Input};

rsx! {
    Input {
        class: "w-full",
        r#type: "text",
        placeholder: "Enter text...",
        value: my_signal(),
        oninput: move |evt: FormEvent| my_signal.set(evt.value()),
    }
    Button {
        variant: ButtonVariant::Primary,
        onclick: move |_| { /* ... */ },
        "Click me"
    }
}
```

## CI/CD

- GitHub Actions (`.github/workflows/deploy.yml`): builds Docker image on push to `main`, pushes to `ghcr.io/typednotes/typednotes`
- Docker build uses multi-stage: Rust builder with `dx bundle --fullstack --release` → Debian slim runtime
- Build requires ~10GB swap (configured in CI) due to WASM compilation memory usage
