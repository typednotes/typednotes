# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

TypedNotes is a cross-platform note-taking application built with Dioxus 0.7.1 that compiles to Web, Desktop, and Mobile from a single Rust codebase.

## Build Commands

```bash
# Serve for development (run from package directory)
cd packages/web && dx serve        # Web
cd packages/desktop && dx serve    # Desktop
cd packages/mobile && dx serve --platform android  # Android
cd packages/mobile && dx serve --platform ios      # iOS

# Update dependencies
make update

# Infrastructure (Scaleway)
make infra-up      # Deploy and save outputs to .env
make infra-down    # Destroy infrastructure
```

## Architecture

```
packages/
├── ui/       # Shared UI components (Hero, Navbar, Echo)
├── api/      # Server functions with #[post]/#[get] macros
├── web/      # Web platform entry point
├── desktop/  # Desktop platform (webview)
└── mobile/   # Mobile platform (iOS/Android)
infra/        # Terraform/OpenTofu for Scaleway
```

**Key pattern:** Shared components in `ui`, server logic in `api`, platform-specific routing in `web`/`desktop`/`mobile`. Each platform wraps the generic `Navbar` component with platform-specific `Link` components.

## Dioxus 0.7 API (Important)

See `AGENTS.md` for complete Dioxus 0.7 reference. Key points:

- **No `cx`, `Scope`, or `use_state`** - these are removed in 0.7
- Use `use_signal(|| value)` for local state, call like function to read: `count()`
- Use `use_memo(move || ...)` for derived values
- Use `use_resource` for async operations, `use_server_future` for fullstack
- Server functions use `#[post("/path")]` / `#[get("/path")]` macros
- Assets: `asset!("/assets/file.png")` - paths relative to project root

## Environment

Runtime configuration in `.env`:
- `DATABASE_URL` - PostgreSQL connection string
- `SDB_ENDPOINT` / `SDB_ID` - Set by `make infra-up`
- OAuth credentials for GitHub authentication
