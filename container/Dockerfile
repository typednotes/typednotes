# Build stage
FROM rust:1.84-bookworm AS builder

# Install dependencies for dx and wasm
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    binaryen \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Add wasm target
RUN rustup target add wasm32-unknown-unknown

# Install dioxus-cli using pre-built binary
RUN curl -fsSL https://github.com/DioxusLabs/dioxus/releases/download/v0.7.1/dx-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xz -C /usr/local/bin

# Create app directory
WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY packages/api/Cargo.toml packages/api/Cargo.toml
COPY packages/ui/Cargo.toml packages/ui/Cargo.toml
COPY packages/web/Cargo.toml packages/web/Cargo.toml
COPY packages/desktop/Cargo.toml packages/desktop/Cargo.toml
COPY packages/mobile/Cargo.toml packages/mobile/Cargo.toml

# Create dummy source files for dependency caching
RUN mkdir -p packages/api/src packages/ui/src packages/web/src packages/desktop/src packages/mobile/src \
    && echo "fn main() {}" > packages/web/src/main.rs \
    && echo "fn main() {}" > packages/desktop/src/main.rs \
    && echo "fn main() {}" > packages/mobile/src/main.rs \
    && echo "" > packages/api/src/lib.rs \
    && echo "" > packages/ui/src/lib.rs

# Build dependencies (this layer is cached)
RUN cargo build --release -p web || true

# Copy actual source code and assets
COPY packages/api/src packages/api/src
COPY packages/api/migrations packages/api/migrations
COPY packages/ui/src packages/ui/src
COPY packages/web/src packages/web/src
COPY packages/web/assets packages/web/assets

# Touch files to invalidate cache
RUN touch packages/api/src/lib.rs packages/ui/src/lib.rs packages/web/src/main.rs

# Build the fullstack app with verbose output
RUN dx bundle --fullstack --release --debug-symbols=false --verbose 2>&1 || \
    (echo "=== Build failed, showing cargo output ===" && \
     cargo build --release -p web --features server 2>&1 && false)

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the built artifacts
COPY --from=builder /app/target/dx/web/release/web/web ./web
COPY --from=builder /app/target/dx/web/release/web/public ./public

# Expose port
EXPOSE 8080

# Set environment variables
ENV RUST_LOG=info
ENV PORT=8080

# Run the server
CMD ["./web"]
