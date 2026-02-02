# Build stage
FROM rust:latest AS builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    binaryen \
    && rm -rf /var/lib/apt/lists/*

# Add wasm target
RUN rustup target add wasm32-unknown-unknown

# Install dioxus-cli
RUN CARGO_BUILD_JOBS=2 cargo install dioxus-cli --locked

WORKDIR /app

# Copy everything
COPY . .

# Build the fullstack app
RUN dx bundle --fullstack --release --debug-symbols=false

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

EXPOSE 8080

ENV RUST_LOG=info
ENV PORT=8080

CMD ["./web"]
