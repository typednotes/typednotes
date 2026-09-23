# syntax=docker/dockerfile:1
#
# The fullstack web app: `dx bundle` builds the wasm client (public/) and the
# server binary (`server` with dx 0.7.10 — older dx named it after the
# package, `web`), which serves both — see Dioxus 0.7's "Deploying".
# Published as ghcr.io/typednotes/typednotes by
# .github/workflows/docker-publish.yml and deployed by typednotes-infra.

# trixie, not bookworm: the prebuilt dx release needs glibc >= 2.39. The
# runtime stage uses the same Debian release, so the server binary's glibc
# requirement is met there too.
FROM docker.io/library/rust:1-trixie AS builder
RUN rustup target add wasm32-unknown-unknown
# dx pinned to the `dioxus` version in Cargo.lock: a CLI newer or older than
# the library can disagree on the wasm-bindgen / asset-manifest formats.
# The prebuilt release binary, not `cargo install`: compiling dx from source
# needs more memory than a small builder has (it was OOM-killed in a 2 GiB VM).
ARG DX_VERSION=0.7.10
RUN curl -fsSL --proto '=https' --tlsv1.2 \
      "https://github.com/DioxusLabs/dioxus/releases/download/v${DX_VERSION}/dx-$(uname -m)-unknown-linux-gnu.tar.gz" \
    | tar xz -C /usr/local/bin dx \
    && dx --version
WORKDIR /app
COPY . .
RUN dx bundle -p web --web --release \
    && cp -r target/dx/web/release/web /out

FROM docker.io/library/debian:trixie-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 typednotes
COPY --from=builder /out /usr/local/app
WORKDIR /usr/local/app
USER typednotes
# Scaleway Serverless Containers set PORT to the declared port; IP must be
# 0.0.0.0 to accept traffic from outside the container.
ENV IP=0.0.0.0 PORT=8080
EXPOSE 8080
# DATABASE_URL is required at runtime (see packages/api/src/db.rs). The
# server never migrates: typednotes-infra applies migrations/ first.
ENTRYPOINT ["/usr/local/app/server"]
